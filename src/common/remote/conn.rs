/**
 * Learn from https://github.com/tokio-rs/console/blob/main/tokio-console/src/conn.rs
 */
use std::{error::Error, pin::Pin, time::Duration};

use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{transport::Channel, Streaming};

use futures::stream::StreamExt;

use crate::api::client_config::ClientConfig;
use crate::common::remote::request::client_request::{
    ConnectionSetupClientRequest, ServerCheckClientRequest,
};
use crate::common::remote::request::Request;
use crate::common::remote::response::Response;
use crate::common::util::*;
use crate::nacos_proto::v2::bi_request_stream_client::BiRequestStreamClient;
use crate::nacos_proto::v2::request_client::RequestClient;
use crate::nacos_proto::v2::Payload;

#[derive(Debug)]
pub struct Connection {
    client_config: ClientConfig,
    state: State,
}

// clippy doesn't like that the "connected" case is much larger than the
// disconnected case, and suggests boxing the connected side's stream.
// however, this is rarely disconnected; it's normally connected. boxing the
// stream just adds a heap pointer dereference, slightly penalizing polling
// the stream in most cases. so, don't listen to clippy on this.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum State {
    Connected {
        conn_id: String,
        client: RequestClient<Channel>,
        bi_client: BiRequestStreamClient<Channel>,
        bi_sender: mpsc::UnboundedSender<Payload>,
        resp_bi_stream: Box<Streaming<Payload>>,
    },
    Disconnected(Duration),
}

/*
macro_rules! with_client {
    ($me:ident, $client:ident, $bi_sender:ident, $block:expr) => ({
        loop {
            match $me.state {
                State::Connected { client: ref mut $client, bi_sender: ref mut $bi_sender, .. } => {
                    match $block {
                        Ok(resp) => break Ok(resp),
                        // If the error is a `h2::Error`, that indicates
                        // something went wrong at the connection level, rather
                        // than the server returning an error code. In that
                        // case, let's try reconnecting...
                        Err(error) if error.source().iter().any(|src| src.is::<h2::Error>()) => {
                            tracing::warn!(
                                error = %error,
                                "connection error sending command"
                            );
                            $me.state = State::Disconnected(Self::BACKOFF);
                        }
                        // Otherwise, return the error.
                        Err(e) => {
                            break Err(e);
                        }
                    }
                }
                State::Disconnected(_) => $me.connect().await,
            }
        }
    })
}
*/

impl Connection {
    const BACKOFF: Duration = Duration::from_millis(500);

    pub(crate) fn new(client_config: ClientConfig) -> Self {
        Self {
            client_config,
            state: State::Disconnected(Duration::from_secs(0)),
        }
    }

    async fn connect(&mut self) {
        const MAX_BACKOFF: Duration = Duration::from_secs(5);

        while let State::Disconnected(backoff) = self.state {
            if backoff == Duration::from_secs(0) {
                tracing::info!(to = %self.client_config.server_addr.as_ref().unwrap(), "connecting");
            } else {
                tracing::info!(reconnect_in = ?backoff, "reconnecting");
                tokio::time::sleep(backoff).await;
            }

            let try_connect = async {
                let target = self.client_config.server_addr.clone().unwrap();
                let tenant = self.client_config.namespace.clone();
                let labels = self.client_config.labels.clone();

                let endpoint = tonic::transport::Endpoint::new(target)?;
                let channel = endpoint.connect().await?;

                let mut client = RequestClient::new(channel.clone());
                let req_payload =
                    payload_helper::build_req_grpc_payload(ServerCheckClientRequest::new());
                let resp_payload = client.request(tonic::Request::new(req_payload)).await?;
                let server_check_response =
                    payload_helper::build_server_response(resp_payload.into_inner()).unwrap();
                let conn_id = server_check_response.get_connection_id();

                let mut bi_client = BiRequestStreamClient::new(channel.clone());
                let (tx, rx) = mpsc::unbounded_channel();
                // send a ConnectionSetupClientRequest
                tx.send(payload_helper::build_req_grpc_payload(
                    ConnectionSetupClientRequest::new(tenant, labels),
                ))
                .unwrap();

                let resp_bi_stream = bi_client
                    .request_bi_stream(UnboundedReceiverStream::from(rx))
                    .await?
                    .into_inner();

                Ok::<State, Box<dyn Error + Send + Sync>>(State::Connected {
                    conn_id: String::from(conn_id.unwrap()),
                    client,
                    bi_client,
                    bi_sender: tx,
                    resp_bi_stream: Box::new(resp_bi_stream),
                })
            };
            self.state = match try_connect.await {
                Ok(connected) => {
                    tracing::debug!("connected successfully!");
                    connected
                }
                Err(error) => {
                    tracing::warn!(%error, "error connecting");
                    let backoff = std::cmp::max(backoff + Self::BACKOFF, MAX_BACKOFF);
                    State::Disconnected(backoff)
                }
            };
        }
    }

    pub(crate) async fn next_payload(&mut self) -> Payload {
        loop {
            match self.state {
                State::Connected {
                    ref mut resp_bi_stream,
                    ..
                } => match Pin::new(resp_bi_stream).next().await {
                    Some(Ok(payload)) => return payload,
                    Some(Err(status)) => {
                        tracing::warn!(%status, "error from stream");
                        self.state = State::Disconnected(Self::BACKOFF);
                    }
                    None => {
                        tracing::error!("stream closed by server");
                        self.state = State::Disconnected(Self::BACKOFF);
                    }
                },
                State::Disconnected(_) => self.connect().await,
            }
        }
    }

    pub(crate) async fn send_req(
        &mut self,
        req: impl Request + serde::Serialize,
    ) -> crate::api::error::Result<Box<dyn Response>> {
        match self.state {
            State::Connected { ref mut client, .. } => {
                let req_payload = payload_helper::build_req_grpc_payload(req);
                let resp_payload = client.request(tonic::Request::new(req_payload)).await?;
                payload_helper::build_server_response(resp_payload.into_inner())
            }
            State::Disconnected(_) => {
                self.connect().await;
                Err(crate::api::error::Error::ClientShutdown(String::from(
                    "Disconnected, please try again.",
                )))
            }
        }
    }

    pub(crate) async fn send_resp(&mut self, resp: impl Response + serde::Serialize) -> () {
        match self.state {
            State::Connected {
                ref mut bi_sender, ..
            } => bi_sender
                .send(payload_helper::build_resp_grpc_payload(resp))
                .unwrap(),
            State::Disconnected(_) => self.connect().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::client_config::ClientConfig;
    use crate::common::remote::conn::Connection;
    use crate::common::util::payload_helper;

    #[tokio::test]
    async fn test_remote_connect() {
        println!("test_remote_connect");
        let mut remote_connect =
            Connection::new(ClientConfig::new().server_addr("http://0.0.0.0:9848".to_string()));
        println!(
            "try to connect {}",
            remote_connect.client_config.server_addr.as_ref().unwrap()
        );
        remote_connect.connect().await
    }

    #[tokio::test]
    async fn test_next_payload() {
        println!("test_next_payload");
        let mut remote_connect =
            Connection::new(ClientConfig::new().server_addr("http://0.0.0.0:9848".to_string()));
        let payload = remote_connect.next_payload().await;
        let server_req = payload_helper::build_server_request(payload).unwrap();
    }

    #[tokio::test]
    async fn test_println() {
        println!("test_println");
    }
}
