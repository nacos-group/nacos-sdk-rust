use std::sync::Arc;

use grpcio::{Environment, ChannelBuilder};

use crate::nacos_proto::v2::{RequestClient, BiRequestStreamClient, Payload};

use tracing::info;

pub(crate) struct  GrpcService {
    name: String,
    request_client: RequestClient,
    bi_request_stream_client: BiRequestStreamClient
}

impl GrpcService {

    fn new(ip: String, port: i32) -> Self {
       let mut addres = format!("{}:{}", ip, port);
       let request_client = Self::init_request_client(addres.as_str());
       let bi_request_stream_client = Self::init_bi_request_stream_client(addres.as_str());
       
       GrpcService { name: addres, request_client, bi_request_stream_client}
    }

    fn init_request_client(address: &str) -> RequestClient {
        info!("init RequestClient: {}", address);
        let env = Arc::new(Environment::new(2));
        let channel = ChannelBuilder::new(env).connect(address);
        RequestClient::new(channel)
    }

    fn init_bi_request_stream_client(address: &str) -> BiRequestStreamClient {
        info!("init BiRequestStreamClient: {}", address);
        let env = Arc::new(Environment::new(2));
        let channel = ChannelBuilder::new(env).connect(address);
        BiRequestStreamClient::new(channel)
    }

    pub fn unary_call_async(&self) {
       let p = Payload::default();
       let any = prost_types::Any::default();
       
    }

    pub fn duplex_streaming(&self) {

    }

    
}


pub(crate) struct GrpcServiceBuilder {
    ip: String,
    port: i32 
}

impl GrpcServiceBuilder {

    pub fn new(ip: String, port: i32) -> Self {
        GrpcServiceBuilder{
            ip,
            port
        }
    }

    pub fn build(self) -> GrpcService {
        GrpcService::new(self.ip, self.port)
    }
}