use crate::api::client_config::ClientConfig;

use super::RemoteClient;

pub(crate) struct GrpcRemoteClient {
    client_config: ClientConfig,
}

impl GrpcRemoteClient {
    pub fn new(client_config: ClientConfig) -> Self {
        Self { client_config }
    }
}

impl RemoteClient for GrpcRemoteClient {}

#[cfg(test)]
mod tests {

    use crate::api::client_config::ClientConfig;
    use crate::common::remote::remote_client::GrpcRemoteClient;

    // #[tokio::test]
    async fn test_grpc_remote_client() {
        let remote_client =
            GrpcRemoteClient::new(ClientConfig::new().server_addr("0.0.0.0:9848".to_string()));
    }
}
