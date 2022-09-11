use std::collections::HashMap;

use tokio::sync::mpsc::{Receiver, Sender};

use crate::api::client_config::ClientConfig;
use crate::common::remote::conn::Connection;
use crate::common::remote::request::server_request::*;
use crate::common::remote::request::*;
use crate::common::remote::response::client_response::*;
use crate::common::util::payload_helper;

pub(crate) struct GrpcRemoteClient {
    pub(crate) client_config: ClientConfig,
    pub(crate) connection: Connection,
    /// (type_url, headers, body_json_str)
    conn_server_req_payload_tx: Sender<(String, HashMap<String, String>, String)>,
    /// (type_url, headers, body_json_str)
    pub(crate) conn_server_req_payload_rx: Receiver<(String, HashMap<String, String>, String)>,
}

impl GrpcRemoteClient {
    pub fn new(client_config: ClientConfig) -> Self {
        let connection = Connection::new(client_config.clone());
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        Self {
            client_config,
            connection,
            conn_server_req_payload_tx: tx,
            conn_server_req_payload_rx: rx,
        }
    }

    /// deal with connection, all logic here.
    pub(crate) async fn deal_with_connection(&mut self) {
        let server_req_payload = self.connection.next_server_req_payload().await;
        let (type_url, headers, body_json_str) = payload_helper::covert_payload(server_req_payload);
        if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&type_url) {
            let de = ClientDetectionServerRequest::from(body_json_str.as_str());
            let de = de.headers(headers);
            self.connection
                .reply_client_resp(ClientDetectionClientResponse::new(
                    de.get_request_id().clone(),
                ))
                .await;
        } else if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&type_url) {
            let de = ConnectResetServerRequest::from(body_json_str.as_str());
            let de = de.headers(headers);
            self.connection
                .reply_client_resp(ConnectResetClientResponse::new(de.get_request_id().clone()))
                .await;
        } else {
            // publish a server_req_payload, conn_server_req_payload_rx receive it once.
            if let Err(_) = self
                .conn_server_req_payload_tx
                .send((type_url, headers, body_json_str))
                .await
            {
                tracing::error!("receiver dropped")
            }
        }
    }

    pub(crate) async fn reply_client_resp(
        &mut self,
        resp: impl crate::common::remote::response::Response + serde::Serialize,
    ) {
        self.connection.reply_client_resp(resp).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use crate::api::client_config::ClientConfig;
    use crate::common::remote::remote_client::GrpcRemoteClient;

    #[tokio::test]
    async fn test_grpc_remote_client() {
        let mut remote_client =
            GrpcRemoteClient::new(ClientConfig::new().server_addr("0.0.0.0:9848".to_string()));
        std::thread::Builder::new()
            .name("grpc-remote-client".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                    .expect("grpc-remote-client runtime initialization failed");

                runtime.block_on(async move {
                    loop {
                        tokio::select! { biased;
                            deal_with_connection = remote_client.deal_with_connection() => {
                                println!("deal_with_connection")
                            },
                        }
                    }
                });
            })
            .expect("grpc-remote-client could not spawn thread");

        sleep(Duration::from_secs(30)).await;
        sleep(Duration::from_secs(30)).await;
    }
}
