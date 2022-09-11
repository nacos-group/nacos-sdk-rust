/**
 * Learn from https://github.com/tokio-rs/console/blob/main/tokio-console/src/conn.rs
 */
use futures::stream::StreamExt;
use futures::SinkExt;

use std::sync::{Arc, Mutex};
use std::{error::Error, time::Duration};

use crate::api::client_config::ClientConfig;
use crate::common::remote::request::client_request::{
    ConnectionSetupClientRequest, ServerCheckClientRequest,
};
use crate::common::remote::request::Request;
use crate::common::remote::response::Response;
use crate::common::util::*;
use crate::nacos_proto::v2::{BiRequestStreamClient, Payload, RequestClient};

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
enum State {
    Connected {
        target: String,
        conn_id: String,
        channel: grpcio::Channel,
        client: RequestClient,
        bi_client: BiRequestStreamClient,
        bi_sender: Arc<Mutex<grpcio::ClientDuplexSender<Payload>>>,
        bi_receiver: Arc<Mutex<grpcio::ClientDuplexReceiver<Payload>>>,
    },
    Disconnected(Duration),
}

impl Connection {
    const BACKOFF: Duration = Duration::from_millis(500);

    pub(crate) fn new(client_config: ClientConfig) -> Self {
        Self {
            client_config,
            state: State::Disconnected(Duration::from_secs(0)),
        }
    }

    pub(crate) async fn connect(&mut self) {
        const MAX_BACKOFF: Duration = Duration::from_secs(5);

        while let State::Disconnected(backoff) = self.state {
            if backoff == Duration::from_secs(0) {
                tracing::info!(to = %self.client_config.server_addr, "connecting");
            } else {
                tracing::info!(reconnect_in = ?backoff, "reconnecting");
                tokio::time::sleep(backoff).await;
            }

            let try_connect = async {
                let target = self.client_config.server_addr.clone();
                let tenant = self.client_config.namespace.clone();
                let labels = self.client_config.labels.clone();

                let env = Arc::new(grpcio::Environment::new(2));
                let channel = grpcio::ChannelBuilder::new(env).connect(target.as_str());

                let client = RequestClient::new(channel.clone());

                let req_payload =
                    payload_helper::build_req_grpc_payload(ServerCheckClientRequest::new());
                let resp_payload = client.request(&req_payload);
                let server_check_response =
                    payload_helper::build_server_response(resp_payload.unwrap()).unwrap();
                let conn_id = server_check_response.get_connection_id();

                let bi_client = BiRequestStreamClient::new(channel.clone());
                let (mut client_sender, client_receiver) = bi_client.request_bi_stream().unwrap();
                // send a ConnectionSetupClientRequest
                client_sender
                    .send((
                        payload_helper::build_req_grpc_payload(ConnectionSetupClientRequest::new(
                            tenant, labels,
                        )),
                        grpcio::WriteFlags::default(),
                    ))
                    .await?;

                Ok::<State, Box<dyn Error + Send + Sync>>(State::Connected {
                    target,
                    conn_id: String::from(conn_id.unwrap()),
                    channel,
                    client,
                    bi_client,
                    bi_sender: Arc::new(Mutex::new(client_sender)),
                    bi_receiver: Arc::new(Mutex::new(client_receiver)),
                })
            };
            self.state = match try_connect.await {
                Ok(connected) => {
                    println!("connected successfully!");
                    tracing::debug!("connected successfully!");
                    connected
                }
                Err(error) => {
                    println!("error connecting {}", error);
                    tracing::warn!(%error, "error connecting");
                    let backoff = std::cmp::min(backoff + Self::BACKOFF, MAX_BACKOFF);
                    State::Disconnected(backoff)
                }
            };
        }
    }

    /// Listen a server_request from server by bi_receiver
    pub(crate) async fn next_server_req_payload(&mut self) -> Payload {
        loop {
            match self.state {
                State::Connected {
                    ref mut bi_receiver,
                    ..
                } => match bi_receiver.to_owned().lock().unwrap().next().await {
                    Some(Ok(payload)) => return payload,
                    Some(Err(status)) => {
                        println!("error from stream {}", status);
                        tracing::warn!(%status, "error from stream");
                        self.state = State::Disconnected(Self::BACKOFF);
                    }
                    None => {
                        println!("stream closed by server");
                        tracing::error!("stream closed by server");
                        self.state = State::Disconnected(Self::BACKOFF);
                    }
                },
                State::Disconnected(_) => {
                    println!("stream closed Disconnected");
                    self.connect().await
                }
            }
        }
    }

    /// Reply a client_resp to server by bi_sender
    pub(crate) async fn reply_client_resp(&mut self, resp: impl Response + serde::Serialize) {
        match self.state {
            State::Connected {
                ref mut bi_sender, ..
            } => bi_sender
                .to_owned()
                .lock()
                .unwrap()
                .send((
                    payload_helper::build_resp_grpc_payload(resp),
                    grpcio::WriteFlags::default(),
                ))
                .await
                .unwrap(),
            State::Disconnected(_) => self.connect().await,
        }
    }

    /// Send a client_req, with get a server_resp
    pub(crate) async fn send_client_req(
        &mut self,
        req: impl Request + serde::Serialize,
    ) -> crate::api::error::Result<Box<Payload>> {
        match self.state {
            State::Connected { ref mut client, .. } => {
                let req_payload = payload_helper::build_req_grpc_payload(req);
                let resp_payload = client.request(&req_payload).unwrap();
                Ok(Box::new(resp_payload))
            }
            State::Disconnected(_) => {
                self.connect().await;
                Err(crate::api::error::Error::ClientShutdown(String::from(
                    "Disconnected, please try again.",
                )))
            }
        }
    }

    /// Get a RequestClient, which use the core channel of connection.
    pub(crate) fn get_client(&mut self) -> crate::api::error::Result<RequestClient> {
        match self.state {
            State::Connected {
                ref mut channel, ..
            } => Ok(RequestClient::new(channel.clone())),
            State::Disconnected(_) => Err(crate::api::error::Error::ClientShutdown(String::from(
                "Disconnected, please try later.",
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::client_config::ClientConfig;
    use crate::common::remote::conn::Connection;
    use crate::common::remote::request::server_request::ClientDetectionServerRequest;
    use crate::common::remote::request::{Request, TYPE_CLIENT_DETECTION_SERVER_REQUEST};
    use crate::common::remote::response::client_response::ClientDetectionClientResponse;
    use crate::common::util::payload_helper;

    #[tokio::test]
    async fn test_remote_connect() {
        tracing_subscriber::fmt::init();
        println!("test_remote_connect");
        let mut remote_connect =
            Connection::new(ClientConfig::new().server_addr("0.0.0.0:9848".to_string()));
        remote_connect.connect().await;
    }

    #[tokio::test]
    async fn test_next_server_request() {
        println!("test_next_payload");
        let mut remote_connect =
            Connection::new(ClientConfig::new().server_addr("0.0.0.0:9848".to_string()));
        let server_req_payload = remote_connect.next_server_req_payload().await;
        let (type_url, headers, body_json_str) = payload_helper::covert_payload(server_req_payload);
        if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&type_url) {
            let de = ClientDetectionServerRequest::from(body_json_str.as_str());
            let de = de.headers(headers);
            remote_connect
                .reply_client_resp(ClientDetectionClientResponse::new(
                    de.get_request_id().clone(),
                ))
                .await;
        }
    }

    #[tokio::test]
    async fn test_println() {
        println!("test_println");
    }
}
