mod client_request;
mod client_response;
mod server_request;
mod server_response;

use crate::api::client_config::ClientConfig;
use crate::api::{ConfigResponse, ConfigService};
use crate::common::remote::conn::Connection;
use crate::common::remote::request::server_request::*;
use crate::common::remote::request::*;
use crate::common::remote::response::client_response::*;
use crate::common::util::payload_helper;
use crate::config::client_request::*;
use crate::config::client_response::*;
use crate::config::server_request::*;
use crate::config::server_response::*;

pub(crate) struct NacosConfigService {
    client_config: ClientConfig,
    client: Option<crate::nacos_proto::v2::RequestClient>,
    conn_thread: Option<std::thread::JoinHandle<()>>,

    /// config listen tx
    config_listen_tx_vec: Vec<std::sync::mpsc::Sender<ConfigResponse>>,
}

impl NacosConfigService {
    pub fn new(client_config: ClientConfig) -> Self {
        Self {
            client_config,
            client: None,
            conn_thread: None,

            config_listen_tx_vec: Vec::new(),
        }
    }

    /// start Once
    pub(crate) async fn start(&mut self) {
        let mut connection = Connection::new(self.client_config.clone());
        connection.connect().await;
        let client = connection.get_client();
        if client.is_ok() {
            self.client = Some(client.unwrap());
        }

        let conn_job = std::thread::Builder::new()
            .name("config-remote-client".into())
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                    .expect("config-remote-client runtime initialization failed");

                runtime.block_on(async move {
                    let (server_req_payload_tx, mut server_req_payload_rx) = tokio::sync::mpsc::channel(128);
                    loop {
                        tokio::select! { biased;
                            // deal with next_server_req_payload, basic conn interaction logic.
                            server_req_payload = connection.next_server_req_payload() => {
                                let (type_url, headers, body_json_str) = payload_helper::covert_payload(server_req_payload);
                                if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&type_url) {
                                    let de = ClientDetectionServerRequest::from(body_json_str.as_str());
                                    let de = de.headers(headers);
                                    connection
                                        .reply_client_resp(ClientDetectionClientResponse::new(de.get_request_id().clone()))
                                        .await;
                                } else if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&type_url) {
                                    let de = ConnectResetServerRequest::from(body_json_str.as_str());
                                    let de = de.headers(headers);
                                    connection
                                        .reply_client_resp(ConnectResetClientResponse::new(de.get_request_id().clone()))
                                        .await;
                                } else {
                                    // publish a server_req_payload, server_req_payload_rx receive it once.
                                    if let Err(_) = server_req_payload_tx.send((type_url, headers, body_json_str)).await {
                                        tracing::error!("receiver dropped")
                                    }
                                }
                            },
                            // receive a server_req from server_req_payload_tx
                            receive_server_req = server_req_payload_rx.recv() => {
                                let (type_url, headers, body_str) = receive_server_req.unwrap();
                                if TYPE_CONFIG_CHANGE_NOTIFY_SERVER_REQUEST.eq(&type_url) {
                                    let server_req = ConfigChangeNotifyServerRequest::from(body_str.as_str());
                                    let server_req = server_req.headers(headers);
                                    connection
                                        .reply_client_resp(ConfigChangeNotifyClientResponse::new(server_req.get_request_id().clone()))
                                        .await;
                                    let tenant = server_req.tenant.or(Some("".to_string())).unwrap();
                                    tracing::info!(
                                        "receiver config change, dataId={},group={},namespace={}",
                                        &server_req.dataId,
                                        &server_req.group,
                                        tenant.clone()
                                    );
                                    println!(
                                        "receiver config change, dataId={},group={},namespace={}",
                                        &server_req.dataId, &server_req.group, tenant.clone()
                                    )
                                    // todo notify config change
                                } else {
                                    tracing::error!("unknown receiver type_url={}", type_url)
                                }
                            },
                        }
                    }
                });
            })
            .expect("config-remote-client could not spawn thread");
        self.conn_thread = Some(conn_job);

        // sleep 100ms, Make sure the link is established.
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

impl ConfigService for NacosConfigService {
    fn get_config(
        &self,
        data_id: String,
        group: String,
        _timeout_ms: u32,
    ) -> crate::api::error::Result<String> {
        if self.client.is_some() {
            let tenant = self.client_config.namespace.clone();
            let req = payload_helper::build_req_grpc_payload(ConfigQueryClientRequest::new(
                data_id, group, tenant,
            ));
            let resp = self.client.as_ref().unwrap().request(&req);
            let (_type_url, _headers, body_str) = payload_helper::covert_payload(resp.unwrap());
            let config_resp = ConfigQueryServerResponse::from(body_str.as_str());
            Ok(String::from(config_resp.get_content()))
        } else {
            Err(crate::api::error::Error::ClientShutdown(String::from(
                "Disconnected, please try later.",
            )))
        }
    }

    fn listen(
        &mut self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<std::sync::mpsc::Receiver<ConfigResponse>> {
        // todo 抽离到统一的发起地方
        let req = ConfigBatchListenClientRequest::new(true);
        let req = req.add_config_listen_context(ConfigListenContext::new(
            data_id,
            group,
            self.client_config.namespace.clone(),
            String::from(""),
        ));
        // todo 抽离到统一的发起地方，取得结果
        let req_payload = payload_helper::build_req_grpc_payload(req);
        let _resp_payload = self
            .client
            .as_ref()
            .expect("Disconnected, please try later.")
            .request(&req_payload)
            .unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        self.config_listen_tx_vec.push(tx);
        return Ok(rx);
    }
}

#[cfg(test)]
mod tests {
    use crate::api::client_config::ClientConfig;
    use crate::api::ConfigService;
    use crate::config::NacosConfigService;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_config_service() {
        let mut config_service = NacosConfigService::new(ClientConfig::new());
        config_service.start().await;
        let config =
            config_service.get_config("hongwen.properties".to_string(), "LOVE".to_string(), 3000);
        println!("get the config {}", config.expect("None"));
        let rx = config_service
            .listen("hongwen.properties".to_string(), "LOVE".to_string())
            .unwrap();
        std::thread::Builder::new()
            .name("config-remote-client".into())
            .spawn(|| {
                for resp in rx {
                    println!("listen the config {}", resp.get_content());
                }
            })
            .expect("config-remote-client could not spawn thread");

        sleep(Duration::from_secs(30)).await;
    }
}
