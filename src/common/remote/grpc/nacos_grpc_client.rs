use std::{collections::HashMap, sync::Arc, time::Duration};
use tower::layer::util::Stack;
use tracing::{instrument, Instrument};

use crate::api::error::Error;
use crate::common::remote::grpc::message::{request::NacosClientAbilities, GrpcMessageData};
use crate::common::remote::grpc::message::{
    GrpcMessage, GrpcMessageBuilder, GrpcRequestMessage, GrpcResponseMessage,
};
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
        let request_headers = request.take_headers();

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
}

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

    pub(crate) fn host(self, host: String) -> Self {
        let grpc_config = self.grpc_config.with_host(host);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn port(self, port: Option<u32>) -> Self {
        let grpc_config = self.grpc_config.with_port(port);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn origin(self, uri: &str) -> Self {
        let grpc_config = self.grpc_config.with_origin(uri);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn user_agent(self, ua: String) -> Self {
        let grpc_config = self.grpc_config.with_user_agent(ua);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn timeout(self, timeout: Duration) -> Self {
        let grpc_config = self.grpc_config.with_timeout(timeout);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn concurrency_limit(self, concurrency_limit: usize) -> Self {
        let grpc_config = self.grpc_config.with_concurrency_limit(concurrency_limit);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn rate_limit(self, rate_limit: (u64, Duration)) -> Self {
        let grpc_config = self.grpc_config.with_rate_limit(rate_limit);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn init_stream_window_size(self, init_stream_window_size: u32) -> Self {
        let grpc_config = self
            .grpc_config
            .with_init_stream_window_size(init_stream_window_size);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn init_connection_window_size(self, init_connection_window_size: u32) -> Self {
        let grpc_config = self
            .grpc_config
            .with_init_connection_window_size(init_connection_window_size);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn tcp_keepalive(self, tcp_keepalive: Duration) -> Self {
        let grpc_config = self.grpc_config.with_tcp_keepalive(tcp_keepalive);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn tcp_nodelay(self, tcp_nodelay: bool) -> Self {
        let grpc_config = self.grpc_config.with_tcp_nodelay(tcp_nodelay);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn http2_keep_alive_interval(self, http2_keep_alive_interval: Duration) -> Self {
        let grpc_config = self
            .grpc_config
            .with_http2_keep_alive_interval(http2_keep_alive_interval);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn http2_keep_alive_timeout(self, http2_keep_alive_timeout: Duration) -> Self {
        let grpc_config = self
            .grpc_config
            .with_http2_keep_alive_timeout(http2_keep_alive_timeout);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn http2_keep_alive_while_idle(self, http2_keep_alive_while_idle: bool) -> Self {
        let grpc_config = self
            .grpc_config
            .with_http2_keep_alive_while_idle(http2_keep_alive_while_idle);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn connect_timeout(self, connect_timeout: Duration) -> Self {
        let grpc_config = self.grpc_config.with_connect_timeout(connect_timeout);
        Self {
            grpc_config,
            ..self
        }
    }

    pub(crate) fn http2_adaptive_window(self, http2_adaptive_window: bool) -> Self {
        let grpc_config = self
            .grpc_config
            .with_http2_adaptive_window(http2_adaptive_window);
        Self {
            grpc_config,
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

    pub(crate) fn build(mut self, id: String) -> NacosGrpcClient {
        self.server_request_handler_map.insert(
            ClientDetectionRequest::identity().to_string(),
            Arc::new(ClientDetectionRequestHandler),
        );

        let send_request = {
            let server_list = PollingServerListService::new(self.server_list);
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
            );

            if let Some(connected_listener) = self.connected_listener {
                connection = connection.connected_listener(connected_listener);
            }

            if let Some(disconnected_listener) = self.disconnected_listener {
                connection = connection.disconnected_listener(disconnected_listener);
            }

            let failover_connection = connection.into_failover_connection(id);
            Arc::new(failover_connection) as Arc<dyn SendRequest + Send + Sync + 'static>
        };
        let app_name = self.app_name;

        NacosGrpcClient {
            app_name,
            send_request,
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
                    .map(|data| data.headers.get(APP_FILED).unwrap().clone())
                    .unwrap();

                app_name.eq("test_app")
            }))
            .returning(|req| {
                let request = GrpcMessage::<HealthCheckRequest>::from_payload(req).unwrap();
                let request = request.into_body();
                let req_id = request.request_id.unwrap();

                let mut response = HealthCheckResponse::default();
                response.request_id = Some(req_id);

                let payload = GrpcMessageBuilder::new(response)
                    .build()
                    .into_payload()
                    .unwrap();
                Ok(payload)
            });

        let nacos_grpc_client = NacosGrpcClient {
            app_name: "test_app".to_string(),
            send_request: Arc::new(mock_send_request),
        };

        let response = nacos_grpc_client
            .send_request::<HealthCheckRequest, HealthCheckResponse>(health_check_request)
            .await;
        let response = response.unwrap();

        assert_eq!(
            "test_health_check_id".to_string(),
            response.request_id.unwrap()
        );
    }
}
