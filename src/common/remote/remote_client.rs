use tokio::sync::mpsc::Sender;

use crate::api::props::ClientProps;
use crate::common::remote::conn::Connection;
use crate::common::remote::request::server_request::*;
use crate::common::remote::request::*;
use crate::common::remote::response::client_response::*;
use crate::common::remote::response::*;
use crate::common::util::payload_helper::PayloadInner;

/// TODO GrpcRemoteClient to be base common client
#[derive(Clone)]
pub(crate) struct GrpcRemoteClient {
    client_props: ClientProps,
    connection: Connection,
    /// PayloadInner {type_url, headers, body_json_str}
    conn_server_req_payload_tx: Sender<PayloadInner>,
}

impl GrpcRemoteClient {
    pub fn new(client_props: ClientProps, server_req_payload_tx: Sender<PayloadInner>) -> Self {
        let connection = Connection::new(client_props.clone());
        Self {
            client_props,
            connection,
            conn_server_req_payload_tx: server_req_payload_tx,
        }
    }

    /// deal with connection, all logic here.
    pub(crate) async fn deal_with_connection(&mut self) {
        let payload_inner = self.connection.next_server_req_payload().await;
        if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&payload_inner.type_url) {
            let de = ClientDetectionServerRequest::from(payload_inner.body_str.as_str());
            let de = de.headers(payload_inner.headers);
            let _ = self
                .connection
                .reply_client_resp(ClientDetectionClientResponse::new(de.request_id().clone()))
                .await;
        } else if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&payload_inner.type_url) {
            let de = ConnectResetServerRequest::from(payload_inner.body_str.as_str());
            let de = de.headers(payload_inner.headers);
            let _ = self
                .connection
                .reply_client_resp(ConnectResetClientResponse::new(de.request_id().clone()))
                .await;
            // todo reset connection
        } else {
            // publish a server_req_payload, conn_server_req_payload_rx receive it once.
            if let Err(_) = self.conn_server_req_payload_tx.send(payload_inner).await {
                tracing::error!("receiver dropped")
            }
        }
    }

    /// Reply a client_resp to server by bi_sender
    pub(crate) async fn reply_client_resp(
        &mut self,
        resp: impl Response + serde::Serialize,
    ) -> crate::api::error::Result<()> {
        self.connection.reply_client_resp(resp).await
    }

    /// Send a client_req, with get a server_resp
    pub(crate) async fn send_client_req(
        &mut self,
        req: impl Request + serde::Serialize,
    ) -> crate::api::error::Result<PayloadInner> {
        self.connection.send_client_req(req).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use crate::api::props::ClientProps;
    use crate::common::remote::remote_client::GrpcRemoteClient;

    // #[tokio::test]
    async fn test_grpc_remote_client() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let (server_req_payload_tx, mut server_req_payload_rx) = tokio::sync::mpsc::channel(128);
        let mut remote_client = GrpcRemoteClient::new(
            ClientProps::new().server_addr("0.0.0.0:9848".to_string()),
            server_req_payload_tx,
        );
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
                                tracing::debug!("deal_with_connection success.")
                            },
                            server_req = server_req_payload_rx.recv() => {
                                let payload_inner = server_req.unwrap();
                                tracing::info!("server_req_payload_inner {:?}", payload_inner)
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
