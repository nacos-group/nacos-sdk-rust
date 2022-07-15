use std::collections::HashMap;

use tonic::{
    async_trait,
    transport::{self, Channel, Endpoint},
};

use crate::api::client_config::ClientConfig;
use crate::api::constants::DEFAULT_SERVER_ADDR;
use crate::nacos_proto::v2::bi_request_stream_client::BiRequestStreamClient;
use crate::nacos_proto::v2::request_client::RequestClient;

use super::RemoteClient;

pub(crate) struct GrpcRemoteClient {
    client: RequestClient<Channel>,
    bi_client: BiRequestStreamClient<Channel>,
}

impl GrpcRemoteClient {
    pub async fn new(client_config: ClientConfig) -> crate::api::error::Result<Self> {
        // TODO init connect and others
        let address = client_config
            .server_addr
            .unwrap_or(String::from(DEFAULT_SERVER_ADDR));
        let endpoint = Endpoint::new(address)?.connect().await?;
        let client = RequestClient::new(endpoint.clone());
        let bi_client = BiRequestStreamClient::new(endpoint.clone());
        let remote_client = Self { client, bi_client };
        Ok(remote_client)
    }
}

#[async_trait]
impl RemoteClient for GrpcRemoteClient {}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::api::client_config::ClientConfig;
    use crate::common::remote::remote_client::GrpcRemoteClient;
    use crate::common::remote::RemoteClient;

    // #[tokio::test]
    async fn test_grpc_remote_client() {
        let mut remote_client = GrpcRemoteClient::new(
            ClientConfig::new().server_addr("http://0.0.0.0:9848".to_string()),
        )
        .await
        .unwrap();
    }
}
