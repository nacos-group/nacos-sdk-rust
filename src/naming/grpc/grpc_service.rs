use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use futures::SinkExt;
use futures::TryStreamExt;
use grpcio::WriteFlags;
use grpcio::{ChannelBuilder, Environment};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use tracing::error;
use tracing::info;
use tracing::warn;

use crate::common::executor;
use crate::nacos_proto::v2::Payload;
use crate::nacos_proto::v2::{BiRequestStreamClient, RequestClient};
use crate::naming::grpc::message::request::ServerCheckRequest;
use crate::naming::grpc::message::response::ServerCheckResponse;
use crate::naming::grpc::message::GrpcMessageBuilder;
use crate::naming::grpc::message::{GrpcMessage, GrpcMessageBody};

type BI_HANDLER = Box<dyn Fn(Payload) -> () + Send + Sync + 'static>;

pub struct GrpcService {
    client_ip: String,
    request_client: RequestClient,
    bi_request_stream_client: BiRequestStreamClient,
    connection_id: String,
    bi_sender: Sender<Payload>,
    bi_hander_map: Arc<RwLock<HashMap<String, BI_HANDLER>>>,
}

impl GrpcService {
    fn new(ip: String, port: i32) -> Self {
        let addres = format!("{}:{}", ip, port);
        let (request_client, bi_request_stream_client, connection_id) = Self::init(addres.as_str());
        let (bi_sender, bi_receiver) = Self::duplex_streaming(&bi_request_stream_client).unwrap();
        GrpcService {
            client_ip: ip,
            request_client,
            bi_request_stream_client,
            connection_id,
            bi_sender,
            bi_hander_map: Arc::new(RwLock::new(HashMap::new())),
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
        mut receiver: Receiver<Payload>,
        hander_map: Arc<RwLock<HashMap<String, BI_HANDLER>>>,
    ) {
        executor::spawn(async move {
            while let Some(mut payload) = receiver.recv().await {
                let metadata = payload.metadata.take();
                if metadata.is_none() {
                    continue;
                }
                let metadata = metadata.unwrap();
                let type_url = &metadata.r#type;
                let read = hander_map.read();
                if let Err(error) = read {
                    error!(
                        "get bi call handler failed, because cannot get read lock. {:?}",
                        error
                    );
                    continue;
                }
                let read = read.unwrap();
                let handler = read.get(type_url);
                if handler.is_none() {
                    continue;
                }
                let handler = handler.unwrap();
                payload.metadata = Some(metadata);
                handler(payload);
            }
        });
    }

    fn check_server(request_client: &RequestClient) -> Option<ServerCheckResponse> {
        info!("check server");
        let request = ServerCheckRequest {};
        let request = GrpcMessageBuilder::new(request).build();
        let request = request.to_payload();
        if request.is_none() {
            error!("request message connot convert to payload");
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
                "response message cannot conver to ServerCheckResponse. {:?}",
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

    pub async fn unary_call_async<R, P>(&self, message: GrpcMessage<R>) -> Option<GrpcMessage<P>>
    where
        R: GrpcMessageBody,
        P: GrpcMessageBody,
    {
        let request_payload = message.to_payload();
        if request_payload.is_none() {
            error!("grpc message convert to payload error");
            return None;
        }
        let request_payload = request_payload.unwrap();

        let response_payload = self.request_client.request_async(&request_payload);

        if let Err(error) = response_payload {
            error!("receive grpc message occur an error. {:?}", error);
            return None;
        }

        let response_payload = response_payload.unwrap().await;

        if let Err(error) = response_payload {
            error!("receive grpc message occur an error. {:?}", error);
            return None;
        }

        let response_payload = response_payload.unwrap();

        let message = GrpcMessage::<P>::from_payload(response_payload);
        if let Err(error) = message {
            error!(
                "convert grpc payload to  message occur an error. {:?}",
                error
            );
            return None;
        }
        Some(message.unwrap())
    }

    pub async fn bi_call(&self, payload: Payload) -> Result<(), SendError<Payload>> {
        self.bi_sender.send(payload).await
    }

    pub fn register_bi_call_handler<T, H>(&self, handler: BI_HANDLER)
    where
        T: GrpcMessageBody,
    {
        let write = self.bi_hander_map.write();
        if let Err(error) = write {
            error!("register call handler failed, cannot get lock. {:?}", error);
            return;
        }
        let mut write = write.unwrap();
        write.insert(T::type_url().to_string(), Box::new(handler));
    }

    fn duplex_streaming(
        bi_request_stream_client: &BiRequestStreamClient,
    ) -> Option<(Sender<Payload>, Receiver<Payload>)> {
        let (req_sender, mut req_recevier) = channel::<Payload>(128);
        let (rsp_sender, rsp_recevier) = channel::<Payload>(128);

        let stream = bi_request_stream_client.request_bi_stream();
        if let Err(error) = stream {
            error!("request bi stream occur an error. {:?}", error);
            return None;
        }
        let (mut sink, mut receiver) = stream.unwrap();

        let send_task = async move {
            while let Some(payload) = req_recevier.recv().await {
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

        Some((req_sender, rsp_recevier))
    }
}

pub struct GrpcServiceBuilder {
    ip: String,
    port: i32,
}

impl GrpcServiceBuilder {
    pub fn new(ip: String, port: i32) -> Self {
        GrpcServiceBuilder { ip, port }
    }

    pub fn build(self) -> GrpcService {
        GrpcService::new(self.ip, self.port)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    pub fn test_build_grpc_server() {
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
}
