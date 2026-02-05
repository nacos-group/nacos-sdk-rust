use std::{collections::HashMap, sync::Arc, time::Duration};
use tower::layer::util::Stack;
use tracing::{Instrument, instrument};

use crate::api::error::Error;
use crate::api::plugin::{AuthPlugin, NoopAuthPlugin, init_auth_plugin};
use crate::common::remote::grpc::message::{
    GrpcMessage, GrpcMessageBuilder, GrpcRequestMessage, GrpcResponseMessage,
};
use crate::common::remote::grpc::message::{GrpcMessageData, request::NacosClientAbilities};
use crate::common::remote::grpc::nacos_grpc_service::DynamicUnaryCallLayerWrapper;

use super::handlers::client_detection_request_handler::ClientDetectionRequestHandler;
use super::message::request::ClientDetectionRequest;
use super::nacos_grpc_connection::{NacosGrpcConnection, SendRequest};
use super::nacos_grpc_service::{
    DynamicBiStreamingCallLayer, DynamicBiStreamingCallLayerWrapper, DynamicUnaryCallLayer,
};
use super::server_list_service::PollingServerListService;
use super::tonic::TonicBuilder;
use super::{config::GrpcConfiguration, nacos_grpc_service::ServerRequestHandler};

const APP_FILED: &str = "app";

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
            request_headers.extend(auth_context.contexts);
        }

        let grpc_request = GrpcMessageBuilder::new(request)
            .header(APP_FILED.to_owned(), self.app_name.clone())
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
type ConnectedListener = Arc<dyn Fn(String) + Send + Sync + 'static>;
type DisconnectedListener = Arc<dyn Fn(String) + Send + Sync + 'static>;

pub(crate) struct NacosGrpcClientBuilder {
    app_name: String,
    client_version: String,
    namespace: String,
    labels: HashMap<String, String>,
    client_abilities: NacosClientAbilities,
    grpc_config: GrpcConfiguration,
    server_request_handler_map: HandlerMap,
    server_list: Vec<String>,
    connected_listener: Option<ConnectedListener>,
    disconnected_listener: Option<DisconnectedListener>,
    unary_call_layer: Option<DynamicUnaryCallLayer>,
    bi_call_layer: Option<DynamicBiStreamingCallLayer>,
    auth_plugin: Arc<dyn AuthPlugin>,
    auth_context: HashMap<String, String>,
    max_retries: Option<u32>,
}

#[allow(dead_code)]
impl NacosGrpcClientBuilder {
    pub(crate) fn new(server_list: Vec<String>) -> Self {
        Self {
            app_name: "unknown".to_owned(),
            client_version: Default::default(),
            namespace: Default::default(),
            labels: Default::default(),
            client_abilities: Default::default(),
            grpc_config: Default::default(),
            server_request_handler_map: Default::default(),
            server_list,
            connected_listener: None,
            disconnected_listener: None,
            unary_call_layer: None,
            bi_call_layer: None,
            auth_context: Default::default(),
            auth_plugin: Arc::new(NoopAuthPlugin::default()),
            max_retries: None,
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

    pub(crate) fn max_retries(mut self, max_retries: Option<u32>) -> Self {
        self.max_retries = max_retries;
        Self { ..self }
    }

    pub(crate) fn support_remote_connection(mut self, enable: bool) -> Self {
        self.client_abilities.support_remote_connection(enable);
        Self { ..self }
    }

    pub(crate) fn support_config_remote_metrics(mut self, enable: bool) -> Self {
        self.client_abilities.support_config_remote_metrics(enable);
        Self { ..self }
    }

    pub(crate) fn support_naming_delta_push(mut self, enable: bool) -> Self {
        self.client_abilities.support_naming_delta_push(enable);
        Self { ..self }
    }

    pub(crate) fn support_naming_remote_metric(mut self, enable: bool) -> Self {
        self.client_abilities.support_naming_remote_metric(enable);
        Self { ..self }
    }

    pub(crate) fn host(mut self, host: String) -> Self {
        self.grpc_config.host = host;
        self
    }

    pub(crate) fn port(mut self, port: Option<u32>) -> Self {
        self.grpc_config.port = port;
        self
    }

    pub(crate) fn origin(mut self, uri: &str) -> Self {
        self.grpc_config = self.grpc_config.with_origin(uri);
        self
    }

    pub(crate) fn user_agent(mut self, ua: String) -> Self {
        self.grpc_config = self.grpc_config.with_user_agent(ua);
        self
    }

    pub(crate) fn timeout(mut self, timeout: Duration) -> Self {
        self.grpc_config.timeout = Some(timeout);
        self
    }

    pub(crate) fn concurrency_limit(mut self, concurrency_limit: usize) -> Self {
        self.grpc_config.concurrency_limit = Some(concurrency_limit);
        self
    }

    pub(crate) fn rate_limit(mut self, rate_limit: (u64, Duration)) -> Self {
        self.grpc_config.rate_limit = Some(rate_limit);
        self
    }

    pub(crate) fn init_stream_window_size(mut self, init_stream_window_size: u32) -> Self {
        self.grpc_config.init_stream_window_size = Some(init_stream_window_size);
        self
    }

    pub(crate) fn init_connection_window_size(mut self, init_connection_window_size: u32) -> Self {
        self.grpc_config.init_connection_window_size = Some(init_connection_window_size);
        self
    }

    pub(crate) fn tcp_keepalive(mut self, tcp_keepalive: Duration) -> Self {
        self.grpc_config.tcp_keepalive = Some(tcp_keepalive);
        self
    }

    pub(crate) fn tcp_nodelay(mut self, tcp_nodelay: bool) -> Self {
        self.grpc_config.tcp_nodelay = tcp_nodelay;
        self
    }

    pub(crate) fn http2_keep_alive_interval(mut self, http2_keep_alive_interval: Duration) -> Self {
        self.grpc_config.http2_keep_alive_interval = Some(http2_keep_alive_interval);
        self
    }

    pub(crate) fn http2_keep_alive_timeout(mut self, http2_keep_alive_timeout: Duration) -> Self {
        self.grpc_config.http2_keep_alive_timeout = Some(http2_keep_alive_timeout);
        self
    }

    pub(crate) fn http2_keep_alive_while_idle(mut self, http2_keep_alive_while_idle: bool) -> Self {
        self.grpc_config.http2_keep_alive_while_idle = Some(http2_keep_alive_while_idle);
        self
    }

    pub(crate) fn connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.grpc_config.connect_timeout = Some(connect_timeout);
        self
    }

    pub(crate) fn http2_adaptive_window(mut self, http2_adaptive_window: bool) -> Self {
        self.grpc_config.http2_adaptive_window = Some(http2_adaptive_window);
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

    pub(crate) fn unary_call_layer(self, layer: DynamicUnaryCallLayer) -> Self {
        let stack = if let Some(unary_call_layer) = self.unary_call_layer {
            Arc::new(Stack::new(
                DynamicUnaryCallLayerWrapper(layer),
                DynamicUnaryCallLayerWrapper(unary_call_layer),
            ))
        } else {
            layer
        };

        Self {
            unary_call_layer: Some(stack),
            ..self
        }
    }

    pub(crate) fn bi_call_layer(self, layer: DynamicBiStreamingCallLayer) -> Self {
        let stack = if let Some(bi_call_layer) = self.bi_call_layer {
            Arc::new(Stack::new(
                DynamicBiStreamingCallLayerWrapper(layer),
                DynamicBiStreamingCallLayerWrapper(bi_call_layer),
            ))
        } else {
            layer
        };

        Self {
            bi_call_layer: Some(stack),
            ..self
        }
    }

    pub(crate) async fn build(mut self, id: String) -> NacosGrpcClient {
        self.server_request_handler_map.insert(
            ClientDetectionRequest::identity().to_string(),
            Arc::new(ClientDetectionRequestHandler),
        );

        let send_request = {
            let server_list = PollingServerListService::new(self.server_list.clone());
            let mut tonic_builder = TonicBuilder::new(self.grpc_config, server_list);
            if let Some(layer) = self.unary_call_layer {
                tonic_builder = tonic_builder.unary_call_layer(layer);
            }

            if let Some(layer) = self.bi_call_layer {
                tonic_builder = tonic_builder.bi_call_layer(layer);
            }

            let mut connection = NacosGrpcConnection::new(
                id.clone(),
                tonic_builder,
                self.server_request_handler_map,
                self.client_version,
                self.namespace,
                self.labels,
                self.client_abilities,
                self.max_retries,
            );

            if let Some(connected_listener) = self.connected_listener {
                connection = connection.connected_listener(connected_listener);
            }

            if let Some(disconnected_listener) = self.disconnected_listener {
                connection = connection.disconnected_listener(disconnected_listener);
            }

            let failover_connection = connection.into_failover_connection(id.clone());
            Arc::new(failover_connection) as Arc<dyn SendRequest + Send + Sync + 'static>
        };

        init_auth_plugin(
            self.auth_plugin.clone(),
            self.server_list.clone(),
            self.auth_context.clone(),
            id,
        )
        .await;

        let app_name = self.app_name;
        let auth_plugin = self.auth_plugin;

        NacosGrpcClient {
            app_name,
            send_request,
            auth_plugin,
        }
    }
}

#[cfg(test)]
pub mod tests {

    use crate::common::remote::grpc::{
        message::{request::HealthCheckRequest, response::HealthCheckResponse},
        nacos_grpc_connection::MockSendRequest,
    };

    use mockall::predicate::*;

    use super::*;

    #[tokio::test]
    pub async fn test_send_request() {
        let mut health_check_request = HealthCheckRequest::default();
        health_check_request.request_id = Some("test_health_check_id".to_string());

        let mut mock_send_request = MockSendRequest::new();
        mock_send_request
            .expect_send_request()
            .with(function(|req: &crate::nacos_proto::v2::Payload| {
                let app_name = &req
                    .metadata
                    .as_ref()
                    .map(|data| {
                        data.headers
                            .get(APP_FILED)
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

                let mut response = HealthCheckResponse::default();
                response.request_id = Some(req_id);

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
