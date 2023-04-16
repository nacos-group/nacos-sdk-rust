use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, instrument, warn, Instrument};

use crate::{
    api::error::Error,
    common::{
        event_bus, executor,
        remote::grpc::{
            events::{ClientInitCompleteEvent, ReconnectedEvent},
            handler::default_handler::DefaultHandler,
            message::{
                request::{ConnectionSetupRequest, ServerCheckRequest},
                GrpcMessageBuilder,
            },
        },
        remote::into_grpc_server_addr,
    },
};

use self::{
    bi_channel::BiChannel,
    grpc_client::GrpcClient,
    handler::{
        client_detection_request_handler::ClientDetectionRequestHandler, GrpcPayloadHandler,
    },
    message::{
        request::{ClientDetectionRequest, HealthCheckRequest},
        response::{HealthCheckResponse, ServerCheckResponse},
        GrpcMessageData, GrpcRequestMessage, GrpcResponseMessage,
    },
    subscribers::{ConnectionHealthCheckEventSubscriber, ReconnectedEventSubscriber},
};
use crate::api::error::Error::ClientUnhealthy;
use crate::api::error::Error::ErrResult;
use crate::api::error::Result;

pub(crate) mod bi_channel;
pub mod events;
pub(crate) mod grpc_client;
pub(crate) mod handler;
pub(crate) mod message;
pub(crate) mod subscribers;

type HandlerMap = HashMap<String, Vec<Arc<dyn GrpcPayloadHandler>>>;
const APP_FILED: &str = "app";
const DEFAULT_CALL_TIME_OUT: u64 = 3000;

pub(crate) struct NacosGrpcClient {
    grpc_client: Arc<GrpcClient>,
    pub(crate) connection_id: String,
    pub(crate) client_id: String,
    bi_handler_map: HandlerMap,
    app_name: String,
    init_semaphore: Semaphore,
}

impl NacosGrpcClient {
    #[instrument(skip_all)]
    pub(crate) async fn new(
        address: String,
        app_name: String,
        grpc_port: Option<u32>,
        client_id: String,
    ) -> Result<Self> {
        let address = into_grpc_server_addr(address.as_str(), true, grpc_port)?;
        let grpc_client = GrpcClient::new(address.as_str(), client_id.clone()).await?;
        let grpc_client = Arc::new(grpc_client);

        let bi_handler_map = HashMap::new();

        Ok(NacosGrpcClient {
            grpc_client,
            connection_id: "".to_string(),
            client_id,
            bi_handler_map,
            app_name,
            init_semaphore: Semaphore::new(1),
        })
    }

    #[instrument(skip_all)]
    pub(crate) fn health_check_task(&self) {
        let client_id = self.client_id.clone();
        let app_name = self.app_name.clone();
        let grpc_client = self.grpc_client.clone();
        executor::spawn(async move {
            loop {
                if grpc_client.is_shutdown() {
                    info!("health check task quit. the grpc client has been shutdown.");
                    break;
                }
                let health_check_request = HealthCheckRequest::default();
                let grpc_message = GrpcMessageBuilder::new(health_check_request)
                    .header(APP_FILED.to_owned(), app_name.clone())
                    .build();

                let response = grpc_client
                    .unary_call_async::<HealthCheckRequest, HealthCheckResponse>(
                        grpc_message,
                        Duration::from_millis(DEFAULT_CALL_TIME_OUT),
                    )
                    .await;
                if let Err(e) = response {
                    match e {
                        crate::api::error::Error::ErrResponse(
                            request_id,
                            ret_code,
                            error_code,
                            message,
                        ) => {
                            error!("health check failed, ready to reinitialize grpc client. request_id:{request_id:?} ret_code:{ret_code} error_code:{error_code} message:{message:?}");
                            // send event
                            event_bus::post(Arc::new(ReconnectedEvent {
                                scope: client_id.clone(),
                            }));
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                        _ => {
                            //just ignore error
                            error!("health check failed. {e}");
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                    }
                }
                let response = response.unwrap().into_body();
                debug!("health check. {response:?}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    #[instrument(skip_all)]
    pub(crate) async fn init(&self, set_up: NacosServerSetUP) -> Result<()> {
        debug!("init nacos grpc client.");
        let semaphore = self.init_semaphore.try_acquire();
        if semaphore.is_err() {
            debug!("the current grpc client is initializing. skip init.");
            return Ok(());
        }

        // got permit , health check.
        let health_check_request = HealthCheckRequest::default();
        let response = self
            .unary_call_async::<HealthCheckRequest, HealthCheckResponse>(health_check_request)
            .await;
        if response.is_ok() {
            debug!("the current client health check pass, don't need to reinitialize");
            return Ok(());
        }

        let mut retry_count = 0;
        let retry_wait_time = 300;

        let mut connection_id = None;

        let bi_channel = self.open_bi_channel().await;
        if let Err(e) = bi_channel {
            error!("set up error: {e:?}");
            return Err(e);
        }

        let bi_channel = bi_channel.unwrap();
        while !bi_channel.is_closed() {
            tokio::time::sleep(Duration::from_millis(
                (retry_wait_time << retry_count).min(1000 * 30),
            ))
            .await;
            retry_count += 1;

            // set up
            debug!("set up grpc connection.");
            let set_up_ret = self.setup(&bi_channel, set_up.clone()).await;
            if let Err(e) = set_up_ret {
                error!("set up error. {e:?}");
                continue;
            }

            // check server
            let check_server_response = self.check_server().await;
            if let Err(e) = check_server_response {
                error!("check server error. {e:?}");
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
            let _ = bi_channel.close().await;
            return Err(ClientUnhealthy(
                "init failed, cannot get connection id".to_string(),
            ));
        }

        let connection_id = connection_id.unwrap();
        info!("new connection id: {connection_id}");

        unsafe {
            #[warn(clippy::cast_ref_to_mut)]
            let mutable_self = &mut *(self as *const Self as *mut Self);
            mutable_self.connection_id = connection_id;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;

        debug!("nacos grpc client init complete.");
        event_bus::post(Arc::new(ClientInitCompleteEvent {
            scope: self.client_id.clone(),
        }));
        Ok(())
    }

    #[instrument(skip_all)]
    async fn open_bi_channel(&self) -> Result<BiChannel> {
        let handler_map = self.bi_handler_map.clone();
        debug!("open bi channel");
        let bi_channel = self
            .grpc_client
            .open_bi_channel(move |mut payload, response_writer| {
                let metadata = payload.metadata.take();
                if metadata.is_none() {
                    return;
                }
                let metadata = metadata.unwrap();
                let type_url = &metadata.r#type;
                let handlers = handler_map.get(type_url);
                if let Some(handlers) = handlers {
                    for handler in handlers {
                        let payload = payload.clone();
                        let handler = handler.clone();
                        let response_writer = response_writer.clone();
                        executor::spawn(
                            async move {
                                handler.hand(response_writer, payload);
                            }
                            .in_current_span(),
                        );
                    }
                } else {
                    let default_handler = DefaultHandler;
                    executor::spawn(
                        async move {
                            default_handler.hand(response_writer.clone(), payload);
                        }
                        .in_current_span(),
                    );
                }
            })
            .await?;

        Ok(bi_channel)
    }

    #[instrument(skip_all)]
    async fn check_server(&self) -> Result<ServerCheckResponse> {
        debug!("check server");
        let request = ServerCheckRequest::new();
        let message = self
            .unary_call_async::<ServerCheckRequest, ServerCheckResponse>(request)
            .await?;
        Ok(message)
    }

    #[instrument(skip_all)]
    async fn setup(&self, bi_channel: &BiChannel, set_up: NacosServerSetUP) -> Result<()> {
        debug!("set up");

        let mut setup_request = ConnectionSetupRequest {
            client_version: set_up.client_version,
            abilities: set_up.abilities,
            tenant: set_up.namespace,
            labels: set_up.labels,
            ..Default::default()
        };

        let request_headers = setup_request.take_headers();

        let grpc_message = GrpcMessageBuilder::new(setup_request)
            .header(APP_FILED.to_owned(), self.app_name.clone())
            .headers(request_headers)
            .build();

        let payload = grpc_message.into_payload()?;

        bi_channel.write(payload).await?;
        Ok(())
    }

    #[instrument(fields(client_id = &self.client_id, conn_id = &self.connection_id), skip(self, request))]
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

        let ret = self
            .grpc_client
            .unary_call_async::<R, P>(grpc_message, Duration::from_millis(DEFAULT_CALL_TIME_OUT))
            .in_current_span()
            .await?;
        let body = ret.into_body();
        if !body.is_success() {
            let message = body
                .message()
                .take()
                .map(|data| data.to_string())
                .unwrap_or_else(|| "error".to_string());
            let error_code = body.error_code();
            error!("error response: {:?},{:?}", message, error_code);
            return Err(crate::api::error::Error::ErrResult(format!(
                "msg:{:?}, code:{:?}",
                message, error_code
            )));
        }
        Ok(body)
    }

    #[instrument(skip_all)]
    pub(crate) async fn register_bi_call_handler(
        &mut self,
        key: String,
        handler: Arc<dyn GrpcPayloadHandler>,
    ) {
        if let Some(vec) = self.bi_handler_map.get_mut(&key) {
            vec.push(handler);
        } else {
            let vec = vec![handler];
            self.bi_handler_map.insert(key, vec);
        }
    }
}

impl Drop for NacosGrpcClient {
    fn drop(&mut self) {
        self.grpc_client.shutdown();
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
    client_id: String,

    address: String,

    grpc_port: Option<u32>,

    labels: HashMap<String, String>,

    client_version: String,

    abilities: NacosClientAbilities,

    namespace: String,

    app_name: String,

    bi_call_handlers: HashMap<String, Vec<Arc<dyn GrpcPayloadHandler>>>,
}

impl NacosGrpcClientBuilder {
    pub(crate) fn new(client_id: String) -> Self {
        let labels = HashMap::<String, String>::new();
        let abilities = NacosClientAbilities::new();

        NacosGrpcClientBuilder {
            address: crate::api::constants::DEFAULT_SERVER_ADDR.to_string(),
            grpc_port: None,
            labels,
            abilities,
            client_version: "".to_string(),
            namespace: "".to_string(),
            app_name: crate::api::constants::UNKNOWN.to_string(),
            bi_call_handlers: Default::default(),
            client_id,
        }
    }

    pub(crate) fn client_id(mut self, client_id: String) -> Self {
        self.client_id = client_id;
        self
    }

    pub(crate) fn address(mut self, address: String) -> Self {
        self.address = address;
        self
    }

    pub(crate) fn grpc_port(mut self, grpc_port: Option<u32>) -> Self {
        self.grpc_port = grpc_port;
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
        call_handler: Arc<dyn GrpcPayloadHandler>,
    ) -> Self
    where
        T: GrpcMessageData,
    {
        let key = T::identity().to_string();
        info!("register_bi_call_handler key={}", key);
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
        let build_fut = async move {
            let mut nacos_grpc_client = NacosGrpcClient::new(
                self.address,
                self.app_name,
                self.grpc_port,
                self.client_id.clone(),
            )
            .await?;
            let server_set_up = NacosServerSetUP {
                labels: self.labels,
                client_version: self.client_version,
                abilities: self.abilities,
                namespace: self.namespace,
            };

            // register grpc payload handler
            for (key, handlers) in self.bi_call_handlers {
                for handler in handlers {
                    nacos_grpc_client
                        .register_bi_call_handler(key.clone(), handler)
                        .await;
                }
            }

            // register default handler
            nacos_grpc_client
                .register_bi_call_handler(
                    ClientDetectionRequest::identity().to_string(),
                    Arc::new(ClientDetectionRequestHandler {
                        client_id: self.client_id.clone(),
                    }),
                )
                .await;

            let nacos_grpc_client = Arc::new(nacos_grpc_client);

            // register event subscriber
            let reconnect_subscriber = ReconnectedEventSubscriber {
                nacos_grpc_client: nacos_grpc_client.clone(),
                set_up_info: server_set_up.clone(),
                scope: self.client_id.clone(),
            };
            let health_check_subscriber = ConnectionHealthCheckEventSubscriber {
                nacos_grpc_client: nacos_grpc_client.clone(),
                scope: self.client_id.clone(),
            };

            event_bus::register(Arc::new(reconnect_subscriber));
            event_bus::register(Arc::new(health_check_subscriber));

            nacos_grpc_client.init(server_set_up).await?;
            nacos_grpc_client.health_check_task();
            Ok::<Arc<NacosGrpcClient>, Error>(nacos_grpc_client)
        }
        .in_current_span();

        let ret = futures::executor::block_on(executor::spawn(build_fut));

        if let Err(e) = ret {
            error!("build client failed. {e}");
            return Err(ErrResult("build client failed.".to_string()));
        }

        ret.unwrap()
    }
}
