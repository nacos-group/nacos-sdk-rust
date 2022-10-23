use std::collections::HashMap;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::{Mutex, RwLock};
use tracing::info;

use crate::common::{event_bus, executor};
use crate::nacos_proto::v2::Payload;
use crate::naming::grpc::handler::{DefaultHandler, GrpcPayloadHandler};

use crate::api::error::Result;
use crate::naming::grpc::message::request::ServerCheckRequest;
use crate::naming::grpc::message::{GrpcMessage, GrpcMessageBuilder, GrpcMessageData};

use super::client_abilities::ClientAbilities;
use super::grpc_client::GrpcClient;
use super::grpc_reconnected_subscriber::GrpcReconnectedEventSubscriber;
use super::handler::NamingPushRequestHandler;
use super::message::request::{ConnectionSetupRequest, NotifySubscriberRequest};
use super::message::response::ServerCheckResponse;

type HandlerMap = Arc<RwLock<HashMap<String, Box<dyn GrpcPayloadHandler>>>>;
pub struct GrpcService {
    grpc_client: GrpcClient,
    pub connection_id: Arc<Mutex<String>>,
    bi_sender: Option<Arc<Sender<Payload>>>,
    bi_handler_map: HandlerMap,
}

#[derive(Clone, Debug)]
pub struct ServerSetUP {
    labels: HashMap<String, String>,
    client_version: String,
    abilities: ClientAbilities,
    namespace: String,
}

impl GrpcService {
    pub async fn new(address: String) -> Self {
        let grpc_client = GrpcClient::new(address.as_str()).await;

        let bi_handler_map = Arc::new(RwLock::new(HashMap::new()));

        GrpcService {
            grpc_client,
            connection_id: Arc::new(Mutex::new("".to_string())),
            bi_sender: None,
            bi_handler_map,
        }
    }

    pub async fn init(&mut self, set_up: ServerSetUP) {
        // check server
        let check_server_response = self.check_server().await.unwrap();
        let connection_id = check_server_response.connection_id;

        let mut self_connection_id = self.connection_id.lock().await;
        *self_connection_id = connection_id;

        // receive bi message
        let bi_sender = self.receive_bi_payload().await;
        self.bi_sender = Some(bi_sender);

        // setup
        self.setup(set_up).await;
    }

    pub async fn receive_bi_payload(&self) -> Arc<Sender<Payload>> {
        let (sender, receiver) = channel::<Payload>(1024);
        let sender = Arc::new(sender);
        let bi_sender = sender.clone();

        let handler_map = self.bi_handler_map.clone();

        let response_receiver = self.grpc_client.streaming_call(receiver).await;
        let mut response_receiver = response_receiver.unwrap();

        executor::spawn(async move {
            while let Some(mut payload) = response_receiver.recv().await {
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

                let sender = sender.clone();

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
        });
        bi_sender
    }

    pub async fn check_server(&self) -> Result<ServerCheckResponse> {
        info!("check server");
        let request = ServerCheckRequest::default();
        let request = GrpcMessageBuilder::new(request).build();

        let message = self
            .grpc_client
            .unary_call_async::<ServerCheckRequest, ServerCheckResponse>(request)
            .await?;
        Ok(message.into_body())
    }

    pub async fn setup(&self, set_up: ServerSetUP) {
        if self.bi_sender.is_none() {
            panic!("the current GrpcServer didn't finish construct.");
        }
        let sender = self.bi_sender.as_ref().unwrap().clone();

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

        let _ = sender.send(message).await;

        let _ = executor::spawn(async move {
            // wait for 300 millis
            sleep(Duration::from_millis(300));
        })
        .await;
    }

    pub async fn unary_call_async<R, P>(&self, message: GrpcMessage<R>) -> Result<GrpcMessage<P>>
    where
        R: GrpcMessageData,
        P: GrpcMessageData,
    {
        self.grpc_client.unary_call_async(message).await
    }

    pub async fn bi_call(&self, payload: Payload) -> Result<()> {
        if self.bi_sender.is_none() {
            panic!("the current GrpcServer didn't finish construct.");
        }
        let bi_sender = self.bi_sender.as_ref().unwrap();
        Ok(bi_sender.send(payload).await?)
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
    pub(crate) fn build(self) -> Arc<GrpcService> {
        futures::executor::block_on(async move {
            let mut grpc_service = GrpcService::new(self.address).await;
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
            let subscriber = GrpcReconnectedEventSubscriber {
                grpc_service: grpc_service.clone(),
                set_up_info: server_set_up.clone(),
            };
            event_bus::register(Arc::new(Box::new(subscriber)));
            grpc_service
        })
    }
}

#[cfg(test)]
mod tests {

    use core::time;
    use std::thread;

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

        let ten_millis = time::Duration::from_secs(300);
        thread::sleep(ten_millis);
    }
}
