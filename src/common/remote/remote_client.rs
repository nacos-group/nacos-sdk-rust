use std::collections::HashMap;

use tonic::{
    async_trait,
    transport::{self, Channel, Endpoint},
};

use crate::nacos_proto::v2::bi_request_stream_client::BiRequestStreamClient;
use crate::nacos_proto::v2::request_client::RequestClient;

use super::RemoteClient;

pub struct GrpcRemoteClient {
    properties: Option<HashMap<String, String>>,
    client: RequestClient<Channel>,
    bi_client: BiRequestStreamClient<Channel>,
}

impl GrpcRemoteClient {
    pub fn new(channel: Channel) -> Self {
        let client = RequestClient::new(channel.clone());
        let bi_client = BiRequestStreamClient::new(channel.clone());
        Self {
            properties: None,
            client,
            bi_client,
        }
    }

    pub async fn connect(
        address: impl TryInto<Endpoint, Error = transport::Error>,
    ) -> crate::common::error::Result<Self> {
        let endpoint = address.try_into()?;
        let client = RequestClient::connect(endpoint.clone()).await?;
        let bi_client = BiRequestStreamClient::connect(endpoint.clone()).await?;
        Ok(Self {
            properties: None,
            client,
            bi_client,
        })
    }
}

#[async_trait]
impl RemoteClient for GrpcRemoteClient {
    fn init(&mut self, properties: HashMap<String, String>) -> () {
        self.properties = Some(properties.clone());
    }
}
