use std::sync::Arc;

use futures::SinkExt;
use futures::TryStreamExt;
use grpcio::WriteFlags;
use grpcio::{ChannelBuilder, Environment};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use tracing::error;
use tracing::info;
use tracing::warn;

use crate::nacos_proto::v2::{BiRequestStreamClient, RequestClient};
use crate::naming::common::executor;
use crate::naming::grpc::message::{GrpcMessage, GrpcMessageBody};

pub(crate) struct GrpcService {
    name: String,
    request_client: RequestClient,
    bi_request_stream_client: BiRequestStreamClient,
}

impl GrpcService {
    fn new(ip: String, port: i32) -> Self {
        let mut addres = format!("{}:{}", ip, port);
        let (request_client, bi_request_stream_client) = Self::init(addres.as_str());
        GrpcService {
            name: addres,
            request_client,
            bi_request_stream_client,
        }
    }

    fn init(address: &str) -> (RequestClient, BiRequestStreamClient) {
        info!("init grpc client: {}", address);
        let env = Arc::new(Environment::new(2));
        let channel = ChannelBuilder::new(env).connect(address);
        let bi_channel = channel.clone();
        let request_client = RequestClient::new(channel);
        let bi_request_stream_client = BiRequestStreamClient::new(bi_channel);

        (request_client, bi_request_stream_client)
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

        GrpcMessage::<P>::from_payload(response_payload)
    }

    pub fn duplex_streaming<R, P>(
        &self,
    ) -> Option<(Sender<GrpcMessage<R>>, Receiver<GrpcMessage<P>>)>
    where
        R: GrpcMessageBody + 'static,
        P: GrpcMessageBody + 'static,
    {
        let (req_sender, mut req_recevier) = channel::<GrpcMessage<R>>(128);
        let (rsp_sender, rsp_recevier) = channel::<GrpcMessage<P>>(128);

        let stream = self.bi_request_stream_client.request_bi_stream();
        if let Err(error) = stream {
            error!("request bi stream occur an error. {:?}", error);
            return None;
        }
        let (mut sink, mut receiver) = stream.unwrap();

        let send_task = async move {
            while let Some(message) = req_recevier.recv().await {
                let payload = message.to_payload();
                if payload.is_none() {
                    warn!("grpc message convert to payload occur an error");
                    continue;
                }
                let payload = payload.unwrap();
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
                let message = GrpcMessage::<P>::from_payload(message);
                if message.is_none() {
                    warn!("payload convert to GrpcMessage occur an error");
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

pub(crate) struct GrpcServiceBuilder {
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
