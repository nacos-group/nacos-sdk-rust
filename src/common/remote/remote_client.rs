use std::collections::HashMap;
use std::sync::Arc;
use tonic::{
    async_trait,
    transport::{Channel, Endpoint},
};

use crate::api::client_config::ClientConfig;
use crate::api::constants::DEFAULT_SERVER_ADDR;
use crate::api::error::Error;
use crate::common::remote::request::client_request::{
    ConnectionSetupClientRequest, ServerCheckClientRequest,
};
use crate::common::util::*;
use crate::nacos_proto::v2::bi_request_stream_client::BiRequestStreamClient;
use crate::nacos_proto::v2::request_client::RequestClient;
use crate::nacos_proto::v2::Payload;

use super::RemoteClient;

pub(crate) struct GrpcRemoteClient {
    client_name: Option<String>,
    tenant: String,
    labels: HashMap<String, String>,
    client: RequestClient<Channel>,
    bi_client: BiRequestStreamClient<Channel>,
    resp_bi_stream: Arc<tonic::codec::Streaming<Payload>>,
}

impl GrpcRemoteClient {
    /// Sets the request_client against.
    pub fn request_client(self, client: RequestClient<Channel>) -> Self {
        GrpcRemoteClient { client, ..self }
    }

    /// Sets the request_client against.
    pub fn bi_client(self, bi_client: BiRequestStreamClient<Channel>) -> Self {
        GrpcRemoteClient { bi_client, ..self }
    }

    /// Sets the resp_bi_stream against.
    pub fn resp_bi_stream(self, resp_bi_stream: Arc<tonic::codec::Streaming<Payload>>) -> Self {
        GrpcRemoteClient {
            resp_bi_stream,
            ..self
        }
    }

    pub async fn new(client_config: ClientConfig) -> crate::api::error::Result<Self> {
        let client_name = client_config.client_name;
        let tenant = client_config.namespace;
        let labels = client_config.labels;
        let address = client_config
            .server_addr
            .unwrap_or(String::from(DEFAULT_SERVER_ADDR));

        let (client, bi_client, resp_bi_stream) =
            Self::connect_to_server(address.clone(), tenant.clone(), labels.clone())
                .await
                .unwrap();

        let resp_bi_stream = Arc::new(resp_bi_stream);

        let mut resp_bi_stream_clone = Arc::clone(&resp_bi_stream);
        /*let thread = tokio::spawn(async move {
            loop {
                while let Some(received) = resp_bi_stream_clone.next().await {
                    let payload = received.unwrap();
                }
            }
        });*/
        // let threads = vec![thread];

        let remote_client = Self {
            client_name,
            tenant,
            labels,
            client,
            bi_client,
            resp_bi_stream,
        };

        Ok(remote_client)
    }

    async fn connect_to_server(
        address: String,
        tenant: String,
        labels: HashMap<String, String>,
    ) -> crate::api::error::Result<(
        RequestClient<Channel>,
        BiRequestStreamClient<Channel>,
        tonic::codec::Streaming<Payload>,
    )> {
        let endpoint = Endpoint::new(address)?;
        let channel = endpoint.connect().await?;
        let mut client = RequestClient::new(channel.clone());

        let req_payload = payload_helper::build_grpc_payload(ServerCheckClientRequest::new());
        let resp_future = client.request(tonic::Request::new(req_payload));
        match resp_future.await {
            Ok(response) => {
                let resp_payload = response.into_inner();
                let server_check_response =
                    payload_helper::build_server_response(resp_payload).unwrap();
                let conn_id = server_check_response.get_connection_id();
                let mut bi_client = BiRequestStreamClient::new(channel.clone());
                let mut resp_bi_stream = bi_client
                    .request_bi_stream(Self::stream_once_connection_setup_request(
                        ConnectionSetupClientRequest::new(tenant, labels),
                    ))
                    .await
                    .unwrap()
                    .into_inner();
                Ok((client, bi_client, resp_bi_stream))
            }
            Err(e) => Err(Error::TonicStatus(e)),
        }
    }

    fn stream_once_connection_setup_request(
        connection_setup_request: ConnectionSetupClientRequest,
    ) -> impl tonic::codegen::futures_core::Stream<Item = Payload> {
        tokio_stream::once(payload_helper::build_grpc_payload(connection_setup_request))
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
