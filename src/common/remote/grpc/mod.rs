use std::{collections::HashMap, sync::Arc, thread::sleep, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::common::{
    event_bus, executor,
    remote::grpc::{
        handler::default_handler::DefaultHandler,
        message::{
            request::{ConnectionSetupRequest, ServerCheckRequest},
            GrpcMessageBuilder,
        },
    },
};

use self::{
    grpc_client::GrpcClient,
    handler::GrpcPayloadHandler,
    message::{
        response::ServerCheckResponse, GrpcMessageData, GrpcRequestMessage, GrpcResponseMessage,
    },
    subscribers::{GrpcConnectHealthCheckEventSubscriber, GrpcReconnectedEventSubscriber},
};
use crate::api::error::Error::ClientUnhealthy;
use crate::api::error::Result;

pub mod events;
pub(crate) mod grpc_client;
pub(crate) mod handler;
pub(crate) mod message;
pub(crate) mod subscribers;

type HandlerMap = Arc<RwLock<HashMap<String, Vec<Arc<Box<dyn GrpcPayloadHandler>>>>>>;
const APP_FILED: &str = "app";

pub(crate) struct NacosGrpcClient {
    grpc_client: RwLock<GrpcClient>,
    pub(crate) connection_id: Arc<RwLock<String>>,
    bi_handler_map: HandlerMap,
    app_name: String,
}

impl NacosGrpcClient {
    pub(crate) async fn new(address: String, app_name: String) -> Result<Self> {
        let grpc_client = GrpcClient::new(address.as_str()).await?;
        let grpc_client = RwLock::new(grpc_client);

        let bi_handler_map = Arc::new(RwLock::new(HashMap::new()));

        Ok(NacosGrpcClient {
            grpc_client,
            connection_id: Arc::new(RwLock::new("".to_string())),
            bi_handler_map,
            app_name,
        })
    }

    pub(crate) async fn switch_server(
        &self,
        server_address: String,
        set_up: NacosServerSetUP,
    ) -> Result<()> {
        // switch server
        info!("switch server starting");
        {
            let mut old_grpc_client = self.grpc_client.write().await;
            old_grpc_client.shutdown().await;

            info!("create a new grpc client.");
            let new_client = GrpcClient::new(server_address.as_str()).await?;
            *old_grpc_client = new_client;
        }
        info!("init new grpc client.");

        self.init(set_up).await
    }

    pub(crate) async fn init(&self, set_up: NacosServerSetUP) -> Result<()> {
        // receive bi message
        self.receive_bi_payload().await;

        let mut connection_id = None;
        for _ in 0..5 {
            // setup
            let setup_ret = self.setup(set_up.clone()).await;
            if let Err(e) = setup_ret {
                error!("setup server error: {:?}", e);
                continue;
            }

            // check server
            let check_server_response = self.check_server().await;
            if let Err(e) = check_server_response {
                error!("check server error. {:?}", e);
                continue;
            }
            let check_server_response = check_server_response.unwrap();
            if !check_server_response.is_success() {
                error!("check server error. {:?}", check_server_response.message);
                continue;
            }

            if check_server_response.connection_id.is_none() {
                error!("init failed, cannot get connection id");
                continue;
            }

            connection_id = check_server_response.connection_id;
            break;
        }

        if connection_id.is_none() {
            return Err(ClientUnhealthy(
                "init failed, cannot get connection id".to_string(),
            ));
        }

        let connection_id = connection_id.unwrap();

        info!("new connection id: {:?}", connection_id);

        {
            let mut self_connection_id = self.connection_id.write().await;
            *self_connection_id = connection_id;
        }

        Ok(())
    }

    pub(crate) async fn receive_bi_payload(&self) {
        info!("starting receive bi payload");
        let handler_map = self.bi_handler_map.clone();

        let grpc_client = self.grpc_client.read().await;
        let (response_sender, mut response_receiver) = grpc_client.streaming_call().await;

        executor::spawn(async move {
            while let Some(payload) = response_receiver.recv().await {
                if let Err(e) = payload {
                    error!(
                        "receive_bi_payload_task receive an error message, close channel. {:?}",
                        e
                    );
                    response_receiver.close();
                    while (response_receiver.recv().await).is_some() {}
                    break;
                }

                let mut payload = payload.unwrap();
                let metadata = payload.metadata.take();
                if metadata.is_none() {
                    continue;
                }
                let metadata = metadata.unwrap();
                let type_url = &metadata.r#type;

                info!(
                    "receive_bi_payload_task receive a message from the server, message type :{:?}",
                    type_url
                );
                let read = handler_map.read().await;
                let handlers = read.get(type_url);

                if let Some(handlers) = handlers {
                    payload.metadata = Some(metadata);
                    for handler in handlers {
                        let sender = response_sender.clone();
                        let handler = handler.clone();
                        let payload = payload.clone();
                        executor::spawn(async move {
                            handler.hand(sender, payload);
                        });
                    }
                } else {
                    let default_handler = DefaultHandler;
                    let sender = response_sender.clone();
                    executor::spawn(async move {
                        default_handler.hand(sender, payload);
                    });
                }
            }

            response_receiver.close();
            while (response_receiver.recv().await).is_some() {}

            warn!("receive_bi_payload_task quit!");
        });
    }

    pub(crate) async fn check_server(&self) -> Result<ServerCheckResponse> {
        info!("check server");
        let request = ServerCheckRequest::default();
        let request = GrpcMessageBuilder::new(request).build();
        let grpc_client = self.grpc_client.read().await;

        let message = grpc_client
            .unary_call_async::<ServerCheckRequest, ServerCheckResponse>(request)
            .await?;
        Ok(message.into_body())
    }

    pub(crate) async fn setup(&self, set_up: NacosServerSetUP) -> Result<()> {
        info!("set up");

        let setup_request = ConnectionSetupRequest {
            client_version: set_up.client_version,
            abilities: set_up.abilities,
            tenant: set_up.namespace,
            labels: set_up.labels,
            ..Default::default()
        };

        self.bi_call(setup_request).await?;
        sleep(Duration::from_millis(400));
        Ok(())
    }

    pub(crate) async fn unary_call_async<R, P>(&self, mut request: R) -> Result<P>
    where
        R: GrpcRequestMessage + 'static,
        P: GrpcResponseMessage + 'static,
    {
        let request_headers = request.take_headers();

        let grpc_message = GrpcMessageBuilder::new(request)
            .header(APP_FILED.to_owned(), self.app_name.clone())
            .headers(request_headers)
            .build();

        let grpc_client = self.grpc_client.read().await;
        let ret = grpc_client.unary_call_async::<R, P>(grpc_message).await?;
        let body = ret.into_body();
        Ok(body)
    }

    pub(crate) async fn bi_call<R>(&self, mut request: R) -> Result<()>
    where
        R: GrpcRequestMessage + 'static,
    {
        let request_headers = request.take_headers();

        let grpc_message = GrpcMessageBuilder::new(request)
            .header(APP_FILED.to_owned(), self.app_name.clone())
            .headers(request_headers)
            .build();

        let grpc_client = self.grpc_client.read().await;
        grpc_client.bi_call(grpc_message).await
    }

    pub(crate) async fn register_bi_call_handler(
        &self,
        key: String,
        handler: Box<dyn GrpcPayloadHandler>,
    ) {
        let mut write = self.bi_handler_map.write().await;
        if let Some(vec) = write.get_mut(&key) {
            vec.push(Arc::new(handler));
        } else {
            let vec = vec![Arc::new(handler)];
            write.insert(key, vec);
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NacosServerSetUP {
    labels: HashMap<String, String>,
    client_version: String,
    abilities: NacosClientAbilities,
    namespace: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct NacosClientAbilities {
    #[serde(rename = "remoteAbility")]
    remote_ability: NacosClientRemoteAbility,

    #[serde(rename = "configAbility")]
    config_ability: NacosClientConfigAbility,

    #[serde(rename = "namingAbility")]
    naming_ability: NacosClientNamingAbility,
}

impl NacosClientAbilities {
    pub(crate) fn new() -> Self {
        NacosClientAbilities {
            remote_ability: NacosClientRemoteAbility::new(),
            config_ability: NacosClientConfigAbility::new(),
            naming_ability: NacosClientNamingAbility::new(),
        }
    }

    pub(crate) fn support_remote_connection(&mut self, enable: bool) {
        self.remote_ability.support_remote_connection(enable);
    }

    pub(crate) fn support_config_remote_metrics(&mut self, enable: bool) {
        self.config_ability.support_remote_metrics(enable);
    }

    pub(crate) fn support_naming_delta_push(&mut self, enable: bool) {
        self.naming_ability.support_delta_push(enable);
    }

    pub(crate) fn support_naming_remote_metric(&mut self, enable: bool) {
        self.naming_ability.support_remote_metric(enable);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct NacosClientRemoteAbility {
    #[serde(rename = "supportRemoteConnection")]
    support_remote_connection: bool,
}

impl NacosClientRemoteAbility {
    fn new() -> Self {
        NacosClientRemoteAbility {
            support_remote_connection: false,
        }
    }

    fn support_remote_connection(&mut self, enable: bool) {
        self.support_remote_connection = enable;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct NacosClientConfigAbility {
    #[serde(rename = "supportRemoteMetrics")]
    support_remote_metrics: bool,
}

impl NacosClientConfigAbility {
    fn new() -> Self {
        NacosClientConfigAbility {
            support_remote_metrics: false,
        }
    }

    fn support_remote_metrics(&mut self, enable: bool) {
        self.support_remote_metrics = enable;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct NacosClientNamingAbility {
    #[serde(rename = "supportDeltaPush")]
    support_delta_push: bool,

    #[serde(rename = "supportRemoteMetric")]
    support_remote_metric: bool,
}

impl NacosClientNamingAbility {
    fn new() -> Self {
        NacosClientNamingAbility {
            support_delta_push: false,
            support_remote_metric: false,
        }
    }

    fn support_delta_push(&mut self, enable: bool) {
        self.support_delta_push = enable;
    }

    fn support_remote_metric(&mut self, enable: bool) {
        self.support_remote_metric = enable;
    }
}

pub(crate) struct NacosGrpcClientBuilder {
    address: String,

    labels: HashMap<String, String>,

    client_version: String,

    abilities: NacosClientAbilities,

    namespace: String,

    app_name: String,

    bi_call_handlers: HashMap<String, Vec<Box<dyn GrpcPayloadHandler>>>,
}

impl NacosGrpcClientBuilder {
    pub(crate) fn new() -> Self {
        let labels = HashMap::<String, String>::new();
        let abilities = NacosClientAbilities::new();

        NacosGrpcClientBuilder {
            address: "localhost:9848".to_string(),
            labels,
            abilities,
            client_version: "".to_string(),
            namespace: "".to_string(),
            app_name: "unknown".to_string(),
            bi_call_handlers: Default::default(),
        }
    }

    pub(crate) fn address(mut self, address: String) -> Self {
        self.address = address;
        self
    }

    pub(crate) fn client_version(mut self, client_version: String) -> Self {
        self.client_version = client_version;
        self
    }

    pub(crate) fn namespace(mut self, namespace: String) -> Self {
        self.namespace = namespace;
        self
    }

    pub(crate) fn add_label(mut self, key: String, value: String) -> Self {
        self.labels.insert(key, value);
        self
    }

    pub(crate) fn add_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
        self
    }

    pub(crate) fn support_remote_connection(mut self, enable: bool) -> Self {
        self.abilities.support_remote_connection(enable);
        self
    }

    pub(crate) fn support_config_remote_metrics(mut self, enable: bool) -> Self {
        self.abilities.support_config_remote_metrics(enable);
        self
    }

    pub(crate) fn support_naming_delta_push(mut self, enable: bool) -> Self {
        self.abilities.support_naming_delta_push(enable);
        self
    }

    pub(crate) fn support_naming_remote_metric(mut self, enable: bool) -> Self {
        self.abilities.support_naming_remote_metric(enable);
        self
    }

    pub(crate) fn app_name(mut self, app_name: String) -> Self {
        self.app_name = app_name;
        self
    }

    pub(crate) fn register_bi_call_handler<T>(
        mut self,
        call_handler: Box<dyn GrpcPayloadHandler>,
    ) -> Self
    where
        T: GrpcMessageData,
    {
        let key = T::identity().to_string();
        debug!("register_bi_call_handler key={}", key);
        let value = self.bi_call_handlers.get_mut(&key);
        if let Some(vec) = value {
            vec.push(call_handler);
        } else {
            let vec = vec![call_handler];
            self.bi_call_handlers.insert(key, vec);
        }
        self
    }

    pub(crate) fn build(self) -> Result<Arc<NacosGrpcClient>> {
        futures::executor::block_on(async move {
            let nacos_grpc_client = NacosGrpcClient::new(self.address, self.app_name).await?;
            let server_set_up = NacosServerSetUP {
                labels: self.labels,
                client_version: self.client_version,
                abilities: self.abilities,
                namespace: self.namespace,
            };
            nacos_grpc_client.init(server_set_up.clone()).await?;

            let nacos_grpc_client = Arc::new(nacos_grpc_client);

            // register event subscriber
            let reconnect_subscriber = GrpcReconnectedEventSubscriber {
                nacos_grpc_client: nacos_grpc_client.clone(),
                set_up_info: server_set_up.clone(),
            };
            let health_check_subscriber = GrpcConnectHealthCheckEventSubscriber {
                nacos_grpc_client: nacos_grpc_client.clone(),
            };

            event_bus::register(Arc::new(Box::new(reconnect_subscriber)));
            event_bus::register(Arc::new(Box::new(health_check_subscriber)));

            // register grpc payload handler
            for (key, handlers) in self.bi_call_handlers {
                for handler in handlers {
                    nacos_grpc_client
                        .register_bi_call_handler(key.clone(), handler)
                        .await;
                }
            }

            Ok(nacos_grpc_client)
        })
    }
}
