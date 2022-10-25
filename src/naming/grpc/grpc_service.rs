use std::collections::HashMap;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::common::{event_bus, executor};
use crate::nacos_proto::v2::Payload;
use crate::naming::grpc::handler::{DefaultHandler, GrpcPayloadHandler};

use crate::api::error::Result;
use crate::naming::grpc::message::request::ServerCheckRequest;
use crate::naming::grpc::message::{GrpcMessageBuilder, GrpcMessageData};

use super::client_abilities::ClientAbilities;
use super::grpc_client::GrpcClient;
use super::handler::NamingPushRequestHandler;
use super::message::request::{ConnectionSetupRequest, NotifySubscriberRequest};
use super::message::response::ServerCheckResponse;
use super::message::{GrpcRequestMessage, GrpcResponseMessage};
use super::subscribers::{GrpcConnectHealthCheckEventSubscriber, GrpcReconnectedEventSubscriber};

type HandlerMap = Arc<RwLock<HashMap<String, Box<dyn GrpcPayloadHandler>>>>;
const APP_FILED: &str = "app";

pub struct GrpcService {
    grpc_client: RwLock<GrpcClient>,
    pub connection_id: Arc<RwLock<String>>,
    bi_handler_map: HandlerMap,
    app_name: String,
}

#[derive(Clone, Debug)]
pub struct ServerSetUP {
    labels: HashMap<String, String>,
    client_version: String,
    abilities: ClientAbilities,
    namespace: String,
}

impl GrpcService {
    pub async fn new(address: String, app_name: String) -> Self {
        let grpc_client = RwLock::new(GrpcClient::new(address.as_str()).await);

        let bi_handler_map = Arc::new(RwLock::new(HashMap::new()));

        GrpcService {
            grpc_client,
            connection_id: Arc::new(RwLock::new("".to_string())),
            bi_handler_map,
            app_name,
        }
    }

    pub async fn switch_server(&self, server_address: String, set_up: ServerSetUP) {
        // switch server
        info!("switch server starting");
        {
            let mut old_grpc_client = self.grpc_client.write().await;
            old_grpc_client.shutdown().await;

            info!("create a new grpc client.");
            let new_client = GrpcClient::new(server_address.as_str()).await;
            *old_grpc_client = new_client;
        }
        info!("init new grpc client.");

        self.init(set_up).await;
    }

    pub async fn init(&self, set_up: ServerSetUP) {
        // receive bi message
        self.receive_bi_payload().await;

        let mut connection_id = None;
        for _ in 0..5 {
            // setup
            self.setup(set_up.clone()).await;

            // check server
            let check_server_response = self.check_server().await;
            if let Err(e) = check_server_response {
                error!("check server error. {:?}", e);
                continue;
            }
            let check_server_response = check_server_response.unwrap();
            if !check_server_response.is_success() {
                error!("check server error. {:?}", check_server_response.message);
                continue;
            }

            if check_server_response.connection_id.is_none() {
                continue;
            }

            connection_id = check_server_response.connection_id;
            break;
        }

        if connection_id.is_none() {
            panic!("init failed. connection id is none");
        }

        let connection_id = connection_id.unwrap();

        info!("new connection id: {:?}", connection_id);

        {
            let mut self_connection_id = self.connection_id.write().await;
            *self_connection_id = connection_id;
        }
    }

    pub async fn receive_bi_payload(&self) {
        info!("receive bi payload");
        let handler_map = self.bi_handler_map.clone();

        let grpc_client = self.grpc_client.read().await;
        let (response_sender, mut response_receiver) = grpc_client.streaming_call().await;

        executor::spawn(async move {
            while let Some(payload) = response_receiver.recv().await {
                if let Err(e) = payload {
                    error!("receive an error message, close channel. {:?}", e);
                    response_receiver.close();
                    while (response_receiver.recv().await).is_some() {}
                    break;
                }

                let mut payload = payload.unwrap();
                let metadata = payload.metadata.take();
                if metadata.is_none() {
                    continue;
                }
                let metadata = metadata.unwrap();
                let type_url = &metadata.r#type;

                info!(
                    "receive push message from the server side, message type :{:?}",
                    type_url
                );
                let read = handler_map.read().await;
                let handler = read.get(type_url);

                let sender = response_sender.clone();

                if let Some(handler) = handler {
                    payload.metadata = Some(metadata);
                    let hand_task = handler.hand(sender, payload);
                    if let Some(hand_task) = hand_task {
                        executor::spawn(hand_task);
                    }
                } else {
                    let default_handler = DefaultHandler;
                    let hand_task = default_handler.hand(sender, payload);
                    if let Some(hand_task) = hand_task {
                        executor::spawn(hand_task);
                    }
                }
            }

            response_receiver.close();
            while (response_receiver.recv().await).is_some() {}

            warn!("receive_bi_payload task quit!");
        });
    }

    pub async fn check_server(&self) -> Result<ServerCheckResponse> {
        info!("check server");
        let request = ServerCheckRequest::default();
        let request = GrpcMessageBuilder::new(request).build();
        let grpc_client = self.grpc_client.read().await;

        let message = grpc_client
            .unary_call_async::<ServerCheckRequest, ServerCheckResponse>(request)
            .await?;
        Ok(message.into_body())
    }

    pub async fn setup(&self, set_up: ServerSetUP) {
        info!("set up");
        let namespace = Some(set_up.namespace);
        let setup_request = ConnectionSetupRequest {
            client_version: set_up.client_version,
            abilities: set_up.abilities,
            namespace,
            labels: set_up.labels,
            ..Default::default()
        };
        let message = GrpcMessageBuilder::new(setup_request).build();
        let message = message.into_payload().unwrap();

        let grpc_client = self.grpc_client.read().await;
        let _ = grpc_client.bi_call(message).await;
        sleep(Duration::from_millis(400));
    }

    pub async fn unary_call_async<R, P>(&self, mut request: R) -> Result<P>
    where
        R: GrpcRequestMessage + 'static,
        P: GrpcResponseMessage + 'static,
    {
        let request_headers = request.take_headers();

        let grpc_message = GrpcMessageBuilder::new(request)
            .header(APP_FILED.to_owned(), self.app_name.clone())
            .headers(request_headers)
            .build();

        let grpc_client = self.grpc_client.read().await;
        let ret = grpc_client.unary_call_async::<R, P>(grpc_message).await?;
        let body = ret.into_body();
        Ok(body)
    }

    pub async fn bi_call(&self, payload: Payload) -> Result<()> {
        let grpc_client = self.grpc_client.read().await;
        grpc_client.bi_call(payload).await
    }

    pub async fn register_bi_call_handler<T>(&self, handler: Box<dyn GrpcPayloadHandler>)
    where
        T: GrpcMessageData,
    {
        let mut write = self.bi_handler_map.write().await;
        write.insert(T::identity().to_string(), handler);
    }
}

pub(crate) struct GrpcServiceBuilder {
    address: String,

    labels: HashMap<String, String>,

    client_version: String,

    abilities: ClientAbilities,

    namespace: String,

    app_name: String,
}

impl GrpcServiceBuilder {
    pub(crate) fn new() -> Self {
        let labels = HashMap::<String, String>::new();
        let abilities = ClientAbilities::new();

        GrpcServiceBuilder {
            address: "localhost:9848".to_string(),
            labels,
            abilities,
            client_version: "".to_string(),
            namespace: "".to_string(),
            app_name: "unknown".to_string(),
        }
    }

    pub(crate) fn address(mut self, address: String) -> Self {
        self.address = address;
        self
    }

    pub(crate) fn client_version(mut self, client_version: String) -> Self {
        self.client_version = client_version;
        self
    }

    pub(crate) fn namespace(mut self, namespace: String) -> Self {
        self.namespace = namespace;
        self
    }

    pub(crate) fn add_label(mut self, key: String, value: String) -> Self {
        self.labels.insert(key, value);
        self
    }

    pub(crate) fn add_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
        self
    }

    pub(crate) fn support_remote_connection(mut self, enable: bool) -> Self {
        self.abilities.support_remote_connection(enable);
        self
    }

    pub(crate) fn support_remote_metrics(mut self, enable: bool) -> Self {
        self.abilities.support_remote_metrics(enable);
        self
    }

    pub(crate) fn support_delta_push(mut self, enable: bool) -> Self {
        self.abilities.support_delta_push(enable);
        self
    }

    pub(crate) fn support_remote_metric(mut self, enable: bool) -> Self {
        self.abilities.support_remote_metric(enable);
        self
    }

    pub(crate) fn app_name(mut self, app_name: String) -> Self {
        self.app_name = app_name;
        self
    }

    pub(crate) fn build(self) -> Arc<GrpcService> {
        futures::executor::block_on(async move {
            let grpc_service = GrpcService::new(self.address, self.app_name).await;
            grpc_service
                .register_bi_call_handler::<NotifySubscriberRequest>(Box::new(
                    NamingPushRequestHandler {
                        event_scope: self.namespace.clone(),
                    },
                ))
                .await;
            let server_set_up = ServerSetUP {
                labels: self.labels,
                client_version: self.client_version,
                abilities: self.abilities,
                namespace: self.namespace,
            };
            grpc_service.init(server_set_up.clone()).await;

            let grpc_service = Arc::new(grpc_service);
            let reconnect_subscriber = GrpcReconnectedEventSubscriber {
                grpc_service: grpc_service.clone(),
                set_up_info: server_set_up.clone(),
            };
            let health_check_subscriber = GrpcConnectHealthCheckEventSubscriber {
                grpc_service: grpc_service.clone(),
            };

            event_bus::register(Arc::new(Box::new(reconnect_subscriber)));
            event_bus::register(Arc::new(Box::new(health_check_subscriber)));
            grpc_service
        })
    }
}

#[cfg(test)]
mod tests {

    use core::time;
    use std::{sync::mpsc::channel, thread};

    use tracing::Level;

    use super::*;

    #[test]
    #[ignore]
    pub fn test_grpc_server_builder() {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_max_level(Level::DEBUG)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .init();

        let _ = GrpcServiceBuilder::new()
            .address("127.0.0.1:7848".to_string())
            .build();

        let ten_millis = time::Duration::from_secs(600);
        thread::sleep(ten_millis);
    }

    #[test]
    #[ignore]
    pub fn test_grpc_server_switch() {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_max_level(Level::DEBUG)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .init();

        let grpc_service = GrpcServiceBuilder::new()
            .address("127.0.0.1:7548".to_string())
            .build();

        let ten_millis = time::Duration::from_secs(15);
        thread::sleep(ten_millis);

        let set_up = create_server_set_up();
        futures::executor::block_on(async move {
            grpc_service
                .switch_server("127.0.0.1:9849".to_string(), set_up)
                .await
        });

        let ten_millis = time::Duration::from_secs(40);
        thread::sleep(ten_millis);
    }

    fn create_server_set_up() -> ServerSetUP {
        let labels = HashMap::<String, String>::new();
        let abilities = ClientAbilities::new();
        ServerSetUP {
            labels,
            abilities,
            client_version: "".to_string(),
            namespace: "".to_string(),
        }
    }
}
