use std::collections::HashMap;

use tonic::{
    async_trait,
    transport::{Channel, Endpoint},
};

use crate::api::client_config::ClientConfig;
use crate::api::constants::DEFAULT_SERVER_ADDR;
use crate::api::error::Error;
use crate::common::remote::request::client_request::ServerCheckClientRequest;
use crate::common::util::*;
use crate::nacos_proto::v2::bi_request_stream_client::BiRequestStreamClient;
use crate::nacos_proto::v2::request_client::RequestClient;

use super::RemoteClient;

pub(crate) struct GrpcRemoteClient {
    client_name: Option<String>,
    labels: HashMap<String, String>,
    client: RequestClient<Channel>,
    bi_client: BiRequestStreamClient<Channel>,
}

impl GrpcRemoteClient {
    pub async fn new(client_config: ClientConfig) -> crate::api::error::Result<Self> {
        // TODO init connect and others
        let client_name = client_config.client_name;
        let labels = client_config.labels;
        let address = client_config
            .server_addr
            .unwrap_or(String::from(DEFAULT_SERVER_ADDR));

        let (client, bi_client) = Self::connect_to_server(address).await.unwrap();

        let remote_client = Self {
            client_name,
            labels,
            client,
            bi_client,
        };
        Ok(remote_client)
    }

    async fn connect_to_server(
        address: String,
    ) -> crate::api::error::Result<(RequestClient<Channel>, BiRequestStreamClient<Channel>)> {
        let endpoint = Endpoint::new(address)?;
        let channel = endpoint.connect().await?;
        let mut client = RequestClient::new(channel.clone());

        let req_payload = payload_helper::build_grpc_payload(ServerCheckClientRequest::new());
        let resp_future = client.request(tonic::Request::new(req_payload));
        match resp_future.await {
            Ok(response) => {
                let resp = response.into_inner();

                let bi_client = BiRequestStreamClient::new(channel.clone());

                Ok((client, bi_client))
            }
            Err(e) => Err(Error::TonicStatus(e)),
        }
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
