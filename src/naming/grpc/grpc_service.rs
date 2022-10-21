use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use futures::SinkExt;
use futures::TryStreamExt;
use grpcio::WriteFlags;
use grpcio::{ChannelBuilder, Environment};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use tracing::error;
use tracing::info;
use tracing::warn;

use crate::common::executor;
use crate::nacos_proto::v2::Payload;
use crate::nacos_proto::v2::{BiRequestStreamClient, RequestClient};
use crate::naming::grpc::handler::GrpcPayloadHandler;
use crate::naming::grpc::message::request::ConnectionSetupRequest;
use crate::naming::grpc::message::request::ServerCheckRequest;
use crate::naming::grpc::message::response::ServerCheckResponse;
use crate::naming::grpc::message::GrpcMessageBuilder;
use crate::naming::grpc::message::{GrpcMessage, GrpcMessageData};

use crate::api::error::Error::GrpcioJoin;
use crate::api::error::Result;

use super::client_abilities::ClientAbilities;
use super::handler::DefaultHandler;
use super::handler::NamingPushRequestHandler;
use super::message::request::NotifySubscriberRequest;

type HandlerMap = Arc<RwLock<HashMap<String, Box<dyn GrpcPayloadHandler>>>>;

pub(crate) struct GrpcService {
    client_address: String,
    request_client: RequestClient,
    bi_request_stream_client: BiRequestStreamClient,
    connection_id: String,
    bi_sender: Arc<Sender<Payload>>,
    bi_handler_map: HandlerMap,
}

impl GrpcService {
    fn new(address: String) -> Self {
        let (request_client, bi_request_stream_client, connection_id) =
            Self::init(address.as_str());

        let (bi_sender, bi_receiver) = Self::duplex_streaming(&bi_request_stream_client).unwrap();
        let bi_sender = Arc::new(bi_sender);

        let bi_handler_map = Arc::new(RwLock::new(HashMap::new()));

        Self::receive_bi_payload(bi_sender.clone(), bi_receiver, bi_handler_map.clone());

        GrpcService {
            client_address: address,
            request_client,
            bi_request_stream_client,
            connection_id,
            bi_sender,
            bi_handler_map,
        }
    }

    fn init(address: &str) -> (RequestClient, BiRequestStreamClient, String) {
        let (request_client, bi_request_stream_client) = Self::build_client(address);
        let response = Self::check_server(&request_client);
        if response.is_none() {
            panic!("Server status check failed!")
        }
        let response = response.unwrap();

        (
            request_client,
            bi_request_stream_client,
            response.connection_id,
        )
    }

    fn build_client(address: &str) -> (RequestClient, BiRequestStreamClient) {
        info!("init grpc client: {}", address);
        let env = Arc::new(Environment::new(2));
        let channel = ChannelBuilder::new(env).connect(address);
        let bi_channel = channel.clone();
        let request_client = RequestClient::new(channel);
        let bi_request_stream_client = BiRequestStreamClient::new(bi_channel);

        (request_client, bi_request_stream_client)
    }

    fn receive_bi_payload(
        bi_sender: Arc<Sender<Payload>>,
        mut receiver: Receiver<Payload>,
        handler_map: HandlerMap,
    ) {
        executor::spawn(async move {
            while let Some(mut payload) = receiver.recv().await {
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
                let read = handler_map.read();
                if let Err(error) = read {
                    error!(
                        "get bi call handler failed, because cannot get read lock. {:?}",
                        error
                    );
                    continue;
                }
                let read = read.unwrap();
                let handler = read.get(type_url);

                if let Some(handler) = handler {
                    payload.metadata = Some(metadata);
                    let hand_task = handler.hand(bi_sender.clone(), payload);
                    if let Some(hand_task) = hand_task {
                        executor::spawn(hand_task);
                    }
                } else {
                    let default_handler = DefaultHandler;
                    let hand_task = default_handler.hand(bi_sender.clone(), payload);
                    if let Some(hand_task) = hand_task {
                        executor::spawn(hand_task);
                    }
                }
            }
        });
    }

    fn check_server(request_client: &RequestClient) -> Option<ServerCheckResponse> {
        info!("check server");
        let request = ServerCheckRequest::default();
        let request = GrpcMessageBuilder::new(request).build();
        let request = request.into_payload();

        if let Err(error) = request {
            error!("check server error:{:?}", error);
            return None;
        }

        let request = request.unwrap();
        let response = request_client.request(&request);
        if let Err(error) = response {
            error!("occur an error connecting to server. {:?}", error);
            return None;
        }
        let response = response.unwrap();
        let response = GrpcMessage::<ServerCheckResponse>::from_payload(response);
        if let Err(error) = response {
            error!(
                "response message cannot convert to ServerCheckResponse. {:?}",
                error
            );
            return None;
        }
        let response = response.unwrap();

        let (body, headers, client_ip) = response.unwrap_all();

        info!(
            "server check: body: {:?}, headers: {:?}, ip: {:?}",
            body, headers, client_ip
        );

        Some(body)
    }

    fn setup(
        &self,
        labels: HashMap<String, String>,
        client_version: String,
        abilities: ClientAbilities,
        namespace: String,
    ) {
        let namespace = Some(namespace);
        let setup_request = ConnectionSetupRequest {
            client_version,
            abilities,
            namespace,
            labels,
            ..Default::default()
        };
        let message = GrpcMessageBuilder::new(setup_request).build();
        let message = message.into_payload().unwrap();
        let sender = self.bi_sender.clone();
        let _ = executor::spawn(async move { sender.send(message).await });

        // wait for 300 millis
        std::thread::sleep(core::time::Duration::from_millis(300));
    }

    fn duplex_streaming(
        bi_request_stream_client: &BiRequestStreamClient,
    ) -> Option<(Sender<Payload>, Receiver<Payload>)> {
        let (req_sender, mut req_receiver) = channel::<Payload>(128);
        let (rsp_sender, rsp_receiver) = channel::<Payload>(128);

        let stream = bi_request_stream_client.request_bi_stream();
        if let Err(error) = stream {
            error!("request bi stream occur an error. {:?}", error);
            return None;
        }
        let (mut sink, mut receiver) = stream.unwrap();

        let send_task = async move {
            while let Some(payload) = req_receiver.recv().await {
                let send_ret = sink.send((payload, WriteFlags::default())).await;
                if let Err(error) = send_ret {
                    error!("send grpc message occur an error. {:?}", error);
                }
            }
            let close_ret = sink.close().await;
            if let Err(error) = close_ret {
                error!("close sink occur an error. {:?}", error);
            }
        };

        let receive_task = async move {
            while let Ok(message) = receiver.try_next().await {
                if message.is_none() {
                    warn!("receive a empty message");
                    continue;
                }
                let message = message.unwrap();
                let send_ret = rsp_sender.send(message).await;
                if let Err(error) = send_ret {
                    error!("send grpc message occur an error. {:?}", error);
                }
            }
        };

        executor::spawn(send_task);
        executor::spawn(receive_task);

        Some((req_sender, rsp_receiver))
    }

    pub(crate) async fn unary_call_async<R, P>(
        &self,
        message: GrpcMessage<R>,
    ) -> Result<GrpcMessage<P>>
    where
        R: GrpcMessageData,
        P: GrpcMessageData,
    {
        let request_payload = message.into_payload();
        if request_payload.is_err() {
            let error = request_payload.unwrap_err();
            error!("unary_call_async error:{:?}", error);
            return Err(error);
        }
        let request_payload = request_payload.unwrap();

        let response_payload = self.request_client.request_async(&request_payload);

        if let Err(error) = response_payload {
            error!("receive grpc message occur an error. {:?}", error);
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap().await;

        if let Err(error) = response_payload {
            error!("receive grpc message occur an error. {:?}", error);
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap();

        let message = GrpcMessage::<P>::from_payload(response_payload);
        if let Err(error) = message {
            error!(
                "convert grpc payload to  message occur an error. {:?}",
                error
            );
            return Err(error);
        }
        Ok(message.unwrap())
    }

    pub(crate) async fn bi_call(&self, payload: Payload) -> Result<()> {
        Ok(self.bi_sender.send(payload).await?)
    }

    pub(crate) fn register_bi_call_handler<T>(&self, handler: Box<dyn GrpcPayloadHandler>)
    where
        T: GrpcMessageData,
    {
        let write = self.bi_handler_map.write();
        if let Err(error) = write {
            error!("register call handler failed, cannot get lock. {:?}", error);
            return;
        }
        let mut write = write.unwrap();
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
    pub(crate) fn build(self) -> GrpcService {
        let grpc_service = GrpcService::new(self.address);
        grpc_service.register_bi_call_handler::<NotifySubscriberRequest>(Box::new(
            NamingPushRequestHandler {
                event_scope: self.namespace.clone(),
            },
        ));
        grpc_service.setup(
            self.labels,
            self.client_version,
            self.abilities,
            self.namespace,
        );
        grpc_service
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    #[ignore]
    pub fn test_check_server() {
        let collector = tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .finish();

        tracing::subscriber::with_default(collector, || {
            let (request_client, _) = GrpcService::build_client("127.0.0.1:9848");
            let response = GrpcService::check_server(&request_client);
            assert!(!response.is_none())
        });
    }

    #[test]
    #[ignore]
    pub fn test_grpc_server_builder() {
        let collector = tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .finish();

        tracing::subscriber::with_default(collector, || {
            let _ = GrpcServiceBuilder::new().build();
        });
    }
}
