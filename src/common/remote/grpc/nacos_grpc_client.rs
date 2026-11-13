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
use super::connection::{ConnectionHandle, ConnectionListener, ExecutorSpawner, SendRequest};
use super::handlers::client_detection_request_handler::ClientDetectionRequestHandler;
use super::message::request::ClientDetectionRequest;
use super::tonic::TonicBuilder;
use super::{config::GrpcConfiguration, nacos_grpc_service::ServerRequestHandler};

const APP_FIELD: &str = "app";

pub(crate) struct NacosGrpcClient {
    app_name: String,
    send_request: Arc<dyn SendRequest + Send + Sync + 'static>,
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
            .send_request
            .send_request(grpc_request)
            .in_current_span()
            .await?;

        let grpc_response = GrpcMessage::<Response>::from_payload(grpc_response)?;
        Ok(grpc_response.into_body())
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

    pub(crate) fn add_label(mut self, key: String, value: String) -> Self {
        self.labels.insert(key, value);
        Self { ..self }
    }

    pub(crate) fn add_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
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

    pub(crate) fn connected_listener(
        mut self,
        listener: impl Fn(String) + Send + Sync + 'static,
    ) -> Self {
        self.connected_listener = Some(Arc::new(listener));
        Self { ..self }
    }

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

        let send_request =
            Arc::new(connection.clone()) as Arc<dyn SendRequest + Send + Sync + 'static>;

        // The 2.3 handshake is the readiness check: `WaitForReady` has already
        // propagated any connection failure before this point.
        waiter.wait().await?;

        if !self.emergency_start {
            let abilities = connection.server_abilities();
            if !abilities
                .get("supportPersistentInstanceByGrpc")
                .copied()
                .unwrap_or(false)
            {
                tracing::warn!(
                    "server does not advertise supportPersistentInstanceByGrpc; persistent naming operations may fail"
                );
            }
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

        Ok(NacosGrpcClient {
            app_name,
            send_request,
            auth_plugin,
        })
    }
}

#[cfg(test)]
pub mod tests {

    use crate::common::remote::grpc::{
        connection::MockSendRequest,
        message::{request::HealthCheckRequest, response::HealthCheckResponse},
    };

    use mockall::predicate::*;

    use super::*;

    #[tokio::test]
    pub async fn test_send_request() {
        let health_check_request = HealthCheckRequest {
            request_id: Some("test_health_check_id".to_string()),
            ..Default::default()
        };

        let mut mock_send_request = MockSendRequest::new();
        mock_send_request
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
            send_request: Arc::new(mock_send_request),
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
}
