use std::{collections::HashMap, sync::Arc};
use tracing::{Instrument, instrument};

use crate::api::error::Error;
use crate::api::plugin::{AuthPlugin, NoopAuthPlugin, init_auth_plugin};
use crate::common::remote::grpc::message::GrpcMessageData;
use crate::common::remote::grpc::message::{
    GrpcMessage, GrpcMessageBuilder, GrpcRequestMessage, GrpcResponseMessage,
};
use crate::common::remote::server_list::{PollingServerListService, ServerListProvider};

use super::connection::supervisor::StartupMode;
use super::connection::{
    Connection, ConnectionHandle, ConnectionListener, ExecutorSpawner,
    abilities::SUPPORT_PERSISTENT_INSTANCE_BY_GRPC,
};
use super::handlers::client_detection_request_handler::ClientDetectionRequestHandler;
use super::message::request::ClientDetectionRequest;
use super::tonic::TonicBuilder;
use super::{config::GrpcConfiguration, nacos_grpc_service::ServerRequestHandler};

const APP_FIELD: &str = "app";

pub(crate) struct NacosGrpcClient {
    app_name: String,
    connection: Arc<dyn Connection>,
    auth_plugin: Arc<dyn AuthPlugin>,
}

impl NacosGrpcClient {
    #[instrument(skip_all)]
    pub(crate) async fn send_request<Request, Response>(
        &self,
        mut request: Request,
    ) -> Result<Response, Error>
    where
        Request: GrpcRequestMessage + 'static,
        Response: GrpcResponseMessage + 'static,
    {
        let mut request_headers = request.take_headers();
        if let Some(resource) = request.request_resource() {
            let auth_context = self.auth_plugin.get_login_identity(resource);
            request_headers.extend(
                auth_context
                    .contexts
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }

        let grpc_request = GrpcMessageBuilder::new(request)
            .header(APP_FIELD.to_owned(), self.app_name.clone())
            .headers(request_headers)
            .build();
        let grpc_request = grpc_request.into_payload()?;

        let grpc_response = self
            .connection
            .send_request(grpc_request)
            .in_current_span()
            .await?;

        let grpc_response = GrpcMessage::<Response>::from_payload(grpc_response)?;
        Ok(grpc_response.into_body())
    }

    /// Reads a server ability from the live connection. The supervisor
    /// refreshes the map after every successful handshake, so this reflects
    /// the server this client is currently talking to.
    // Shared connection accessor; the AI feature is its current caller.
    #[cfg_attr(not(feature = "ai"), allow(dead_code))]
    pub(crate) async fn server_ability(&self, name: &str) -> Result<Option<bool>, Error> {
        self.connection.server_ability(name).await
    }

    pub(crate) fn connection_id(&self) -> Option<String> {
        self.connection.connection_id()
    }
}

type HandlerMap = HashMap<String, Arc<dyn ServerRequestHandler>>;

pub(crate) struct NacosGrpcClientBuilder {
    app_name: String,
    client_version: String,
    namespace: String,
    labels: HashMap<String, String>,
    grpc_config: GrpcConfiguration,
    server_request_handler_map: HandlerMap,
    server_list_provider: Arc<dyn ServerListProvider>,
    connected_listener: Option<ConnectionListener>,
    disconnected_listener: Option<ConnectionListener>,
    auth_plugin: Arc<dyn AuthPlugin>,
    auth_context: HashMap<String, String>,
    emergency_start: bool,
}

impl NacosGrpcClientBuilder {
    pub(crate) fn new(server_list_provider: Arc<dyn ServerListProvider>) -> Self {
        Self {
            app_name: "unknown".to_owned(),
            client_version: Default::default(),
            namespace: Default::default(),
            labels: Default::default(),
            grpc_config: Default::default(),
            server_request_handler_map: Default::default(),
            server_list_provider,
            connected_listener: None,
            disconnected_listener: None,
            auth_context: Default::default(),
            auth_plugin: Arc::new(NoopAuthPlugin::default()),
            emergency_start: false,
        }
    }

    pub(crate) fn app_name(self, app_name: String) -> Self {
        Self { app_name, ..self }
    }

    pub(crate) fn client_version(self, client_version: String) -> Self {
        Self {
            client_version,
            ..self
        }
    }

    pub(crate) fn namespace(self, namespace: String) -> Self {
        Self { namespace, ..self }
    }

    pub(crate) fn add_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
        Self { ..self }
    }

    /// Adds the `source` and `module` labels every client sends.
    pub(crate) fn add_default_labels(mut self, module: &str) -> Self {
        self.labels.insert(
            crate::api::constants::common_remote::LABEL_SOURCE.to_owned(),
            crate::api::constants::common_remote::LABEL_SOURCE_SDK.to_owned(),
        );
        self.labels.insert(
            crate::api::constants::common_remote::LABEL_MODULE.to_owned(),
            module.to_owned(),
        );
        Self { ..self }
    }

    pub(crate) fn emergency_start(mut self, emergency_start: bool) -> Self {
        self.emergency_start = emergency_start;
        Self { ..self }
    }

    pub(crate) fn port(mut self, port: Option<u16>) -> Self {
        self.grpc_config.port = port;
        self
    }

    pub(crate) fn auth_plugin(self, auth_plugin: Arc<dyn AuthPlugin>) -> Self {
        Self {
            auth_plugin,
            ..self
        }
    }

    pub(crate) fn auth_context(self, auth_context: HashMap<String, String>) -> Self {
        Self {
            auth_context,
            ..self
        }
    }

    pub(crate) fn register_server_request_handler<T: GrpcMessageData>(
        mut self,
        handler: Arc<dyn ServerRequestHandler>,
    ) -> Self {
        self.server_request_handler_map
            .insert(T::identity().to_string(), handler);
        Self { ..self }
    }

    // Only the naming and ai services wire connection listeners; unused with `config` alone.
    #[allow(dead_code)]
    pub(crate) fn connected_listener(
        mut self,
        listener: impl Fn(String) + Send + Sync + 'static,
    ) -> Self {
        self.connected_listener = Some(Arc::new(listener));
        Self { ..self }
    }

    // Only the naming service wires connection listeners; unused with `config` alone.
    #[allow(dead_code)]
    pub(crate) fn disconnected_listener(
        mut self,
        listener: impl Fn(String) + Send + Sync + 'static,
    ) -> Self {
        self.disconnected_listener = Some(Arc::new(listener));
        Self { ..self }
    }

    pub(crate) async fn build(mut self, id: String) -> Result<NacosGrpcClient, Error> {
        self.server_request_handler_map.insert(
            ClientDetectionRequest::identity().to_string(),
            Arc::new(ClientDetectionRequestHandler),
        );

        let server_list =
            PollingServerListService::from_provider(self.server_list_provider.clone()).await;

        let settings = crate::common::remote::grpc::connection::ConnectionSettings {
            client_version: self.client_version,
            namespace: self.namespace,
            labels: self.labels,
            ..Default::default()
        };

        let (connection, waiter) = ConnectionHandle::start_with(
            id.clone(),
            TonicBuilder::new(self.grpc_config, server_list),
            self.server_request_handler_map,
            settings,
            if self.emergency_start {
                StartupMode::Background
            } else {
                StartupMode::WaitForReady { max_attempts: 3 }
            },
            self.connected_listener,
            self.disconnected_listener,
            Arc::new(ExecutorSpawner),
        );

        let connection: Arc<dyn Connection> = Arc::new(connection);

        // The 2.3 handshake is the readiness check: `WaitForReady` has already
        // propagated any connection failure before this point.
        waiter.wait().await?;

        if !self.emergency_start
            && !connection
                .server_ability(SUPPORT_PERSISTENT_INSTANCE_BY_GRPC)
                .await?
                .unwrap_or(false)
        {
            tracing::warn!(
                "server does not advertise supportPersistentInstanceByGrpc; persistent naming operations may fail"
            );
        }

        init_auth_plugin(
            self.auth_plugin.clone(),
            self.server_list_provider.clone(),
            self.auth_context.clone(),
            id,
        )
        .await;

        let app_name = self.app_name;
        let auth_plugin = self.auth_plugin;

        let client = NacosGrpcClient {
            app_name,
            connection,
            auth_plugin,
        };
        if let Some(connection_id) = client.connection_id() {
            tracing::debug!(connection_id = %connection_id, "nacos grpc client built");
        }
        Ok(client)
    }
}

#[cfg(test)]
pub mod tests {

    use crate::common::remote::grpc::connection::MockConnection;
    use crate::common::remote::grpc::connection::abilities::SUPPORT_PERSISTENT_INSTANCE_BY_GRPC;
    use crate::common::remote::grpc::message::{
        request::HealthCheckRequest, response::HealthCheckResponse,
    };

    use mockall::predicate::*;

    use super::*;

    #[tokio::test]
    pub async fn test_send_request() {
        let health_check_request = HealthCheckRequest {
            request_id: Some("test_health_check_id".to_string()),
            ..Default::default()
        };

        let mut mock_connection = MockConnection::new();
        mock_connection
            .expect_send_request()
            .with(function(|req: &crate::nacos_proto::v2::Payload| {
                let app_name = &req
                    .metadata
                    .as_ref()
                    .map(|data| {
                        data.headers
                            .get(APP_FIELD)
                            .expect("APP field should exist in headers")
                            .clone()
                    })
                    .expect("APP field extraction should not fail");

                app_name.eq("test_app")
            }))
            .returning(|req| {
                let request = GrpcMessage::<HealthCheckRequest>::from_payload(req)
                    .expect("Payload should deserialize to HealthCheckRequest");
                let request = request.into_body();
                let req_id = request
                    .request_id
                    .expect("Request ID should exist in the deserialized request");

                let response = HealthCheckResponse {
                    request_id: Some(req_id),
                    ..Default::default()
                };

                let payload = GrpcMessageBuilder::new(response)
                    .build()
                    .into_payload()
                    .expect("GRPC message should build into payload");
                Ok(payload)
            });

        let nacos_grpc_client = NacosGrpcClient {
            app_name: "test_app".to_string(),
            connection: Arc::new(mock_connection),
            auth_plugin: Arc::new(NoopAuthPlugin::default()),
        };

        let response = nacos_grpc_client
            .send_request::<HealthCheckRequest, HealthCheckResponse>(health_check_request)
            .await;
        let response = response.expect("Health check response should succeed");

        assert_eq!(
            "test_health_check_id".to_string(),
            response
                .request_id
                .expect("Response request ID should exist")
        );
    }

    #[tokio::test]
    async fn test_server_ability_delegates_to_connection() {
        let mut mock_connection = MockConnection::new();
        mock_connection
            .expect_server_ability()
            .with(eq(SUPPORT_PERSISTENT_INSTANCE_BY_GRPC))
            .returning(|_| Ok(Some(true)));

        let nacos_grpc_client = NacosGrpcClient {
            app_name: "test_app".to_string(),
            connection: Arc::new(mock_connection),
            auth_plugin: Arc::new(NoopAuthPlugin::default()),
        };

        assert_eq!(
            nacos_grpc_client
                .server_ability(SUPPORT_PERSISTENT_INSTANCE_BY_GRPC)
                .await
                .expect("ability read"),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_connection_id_delegates_to_connection() {
        let mut mock_connection = MockConnection::new();
        mock_connection
            .expect_connection_id()
            .returning(|| Some("c-1".to_string()));

        let nacos_grpc_client = NacosGrpcClient {
            app_name: "test_app".to_string(),
            connection: Arc::new(mock_connection),
            auth_plugin: Arc::new(NoopAuthPlugin::default()),
        };

        assert_eq!(nacos_grpc_client.connection_id().as_deref(), Some("c-1"));
    }
}
