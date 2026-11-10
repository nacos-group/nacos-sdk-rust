use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::Poll;
use std::time::Duration;
use std::{collections::HashMap, pin::Pin, sync::Arc};

use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use futures::{Future, future};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::sleep;
use tower::buffer::Buffer;
use tower::{MakeService, Service};
use tracing::{Instrument, debug, debug_span, error, info, instrument, warn};

use crate::api::error::Error::ErrResult;
use crate::api::error::Error::GrpcBufferRequest;
use crate::common::executor;
use crate::common::remote::grpc::handlers::default_handler::DefaultHandler;
use crate::common::remote::grpc::message::request::{
    ConnectionSetupRequest, HealthCheckRequest, NacosClientAbilities, ServerCheckRequest,
};
use crate::common::remote::grpc::message::response::{HealthCheckResponse, ServerCheckResponse};
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder, GrpcRequestMessage};
use crate::common::remote::grpc::nacos_grpc_service::GrpcStream;
use crate::common::remote::grpc::utils;
use crate::{api::error::Error, nacos_proto::v2::Payload};

use super::nacos_grpc_service::NacosGrpcCall;
use super::nacos_grpc_service::ServerRequestHandler;

type ConnectedListener = Arc<dyn Fn(String) + Send + Sync + 'static>;
type DisconnectedListener = Arc<dyn Fn(String) + Send + Sync + 'static>;

type HandlerMap = HashMap<String, Arc<dyn ServerRequestHandler>>;
const MAX_RETRY: u32 = 6;

// Initialization gives up after this many failed attempts, matching the
// legacy `max_retries = None` default. Runtime reconnects stay unbounded.
const MAX_INIT_RETRIES: u32 = 1;

const HEALTH_CHECK_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(5);

const HEALTH_CHECK_RETRY_TIMES: u32 = 3;

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn sleep_time(retry_count: u32) -> u32 {
    if retry_count > MAX_RETRY {
        1 << MAX_RETRY
    } else {
        1 << retry_count
    }
}

pub(crate) struct NacosGrpcConnection<M>
where
    M: MakeService<(), NacosGrpcCall>,
{
    id: String,
    client_version: String,
    namespace: String,
    labels: HashMap<String, String>,
    client_abilities: NacosClientAbilities,
    handler_map: Arc<HandlerMap>,
    mk_service: M,
    state: State<M::Future, M::Service>,
    health: Arc<AtomicBool>,
    last_active: Arc<AtomicU64>,
    connection_id: Option<String>,
    retry_count: u32,
    connection_id_watcher: (
        watch::Sender<Option<String>>,
        watch::Receiver<Option<String>>,
    ),
    is_initialized: bool,
}

impl<M> NacosGrpcConnection<M>
where
    M: MakeService<(), NacosGrpcCall, Error = Error, MakeError = Error> + Send,
    M::Error: Send + 'static,
    M::Response: Send + 'static,
    M::Future: Send + 'static,
    M::Service: Send + 'static,
    <M::Service as Service<NacosGrpcCall>>::Future: Send + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: String,
        mk_service: M,
        handler_map: HandlerMap,
        client_version: String,
        namespace: String,
        labels: HashMap<String, String>,
        client_abilities: NacosClientAbilities,
    ) -> Self {
        let connection_id_watcher = watch::channel(None);

        Self {
            id,
            handler_map: Arc::new(handler_map),
            mk_service,
            client_version,
            namespace,
            labels,
            client_abilities,
            state: State::Idle,
            health: Arc::new(AtomicBool::new(false)),
            last_active: Arc::new(AtomicU64::new(now_millis())),
            connection_id: None,
            retry_count: 0,
            connection_id_watcher,
            is_initialized: false,
        }
    }

    fn enter_retry(&mut self, cause: RetryCause) {
        self.retry_count += 1;
        let sleep_time = sleep_time(self.retry_count);
        match cause {
            RetryCause::Connect(e) => error!(
                "create connection error, this operate will be retry after {} sec, retry count:{}. {}",
                sleep_time, self.retry_count, e
            ),
            RetryCause::Init(e) => error!(
                "initializing connection error, this operate will be retry after {} sec, retry count:{}. {}",
                sleep_time, self.retry_count, e
            ),
            RetryCause::Unhealthy => error!(
                "the connection {:?} is not in health, destroy connection and retry, this operate will be retry after {} sec, retry count:{}.",
                self.connection_id, sleep_time, self.retry_count
            ),
            RetryCause::NotReady(e) => error!(
                "connection {:?} not ready, destroy connection and retry, this operate will be retry after {} sec, retry count:{}. {}",
                self.connection_id, sleep_time, self.retry_count, e
            ),
        }
        self.state = State::Retry(Box::pin(sleep(Duration::from_secs(sleep_time as u64))));
    }

    pub(crate) fn connected_listener(self, listener: ConnectedListener) -> Self {
        self.connection_status_listener(true, listener)
    }

    pub(crate) fn disconnected_listener(self, listener: DisconnectedListener) -> Self {
        self.connection_status_listener(false, listener)
    }

    fn connection_status_listener(
        self,
        connected: bool,
        listener: Arc<dyn Fn(String) + Send + Sync + 'static>,
    ) -> Self {
        let mut rx = self.connection_id_watcher.1.clone();
        let watch_fu = async move {
            let mut previous_id: Option<String> = None;
            while rx.changed().await.is_ok() {
                let current_id = rx.borrow().clone();
                if previous_id.is_some() != current_id.is_some() {
                    let id = if connected {
                        current_id.as_ref()
                    } else {
                        previous_id.as_ref()
                    };
                    if let Some(id) = id {
                        listener(id.clone());
                    }
                }
                previous_id = current_id;
            }
            debug!("connection status listener quit, connected={connected}");
        };
        executor::spawn(watch_fu);
        self
    }

    pub(crate) fn into_failover_connection(
        self,
        id: String,
    ) -> FailoverConnection<NacosGrpcConnection<M>> {
        let svc_health = self.health.clone();
        let svc_last_active = self.last_active.clone();
        FailoverConnection::new(id, self, svc_health, svc_last_active)
    }

    #[allow(clippy::too_many_arguments)]
    async fn init_connection(
        mut service: M::Service,
        client_version: String,
        namespace: String,
        labels: HashMap<String, String>,
        client_abilities: NacosClientAbilities,
        handler_map: Arc<HandlerMap>,
        health: Arc<AtomicBool>,
        last_active: Arc<AtomicU64>,
    ) -> Result<(M::Service, String), Error> {
        // setup
        let conn_id_sender = NacosGrpcConnection::<M>::setup(
            handler_map,
            &mut service,
            health,
            last_active,
            client_version,
            namespace,
            labels,
            client_abilities,
        )
        .in_current_span()
        .await?;

        // connection health check
        for i in 0..4 {
            let health_check = NacosGrpcConnection::<M>::connection_health_check(&mut service)
                .in_current_span()
                .await;
            if health_check.is_err() {
                sleep(Duration::from_millis(300 << i)).await;
                continue;
            }
            break;
        }

        // check server
        let connection_id = NacosGrpcConnection::<M>::check_server(&mut service)
            .in_current_span()
            .await?;

        let conn_id_send_ret = conn_id_sender.send(connection_id.clone());
        if let Err(e) = conn_id_send_ret {
            // maybe error? perhaps.
            error!(
                "send connection id to bi stream task occur an error. please check connection state. {e}"
            );
            return Err(ErrResult("the bi stream task has already quit, because connection id sender send id occur an error".to_string()));
        }

        // set connection id
        Ok((service, connection_id))
    }

    #[allow(clippy::too_many_arguments)]
    async fn setup(
        server_stream_handlers: Arc<HandlerMap>,
        service: &mut M::Service,
        health: Arc<AtomicBool>,
        last_active: Arc<AtomicU64>,
        client_version: String,
        namespace: String,
        labels: HashMap<String, String>,
        client_abilities: NacosClientAbilities,
    ) -> Result<oneshot::Sender<String>, Error> {
        info!("setup connection");

        let setup_request = ConnectionSetupRequest {
            client_version,
            labels,
            tenant: namespace,
            abilities: client_abilities,
            ..Default::default()
        };

        let (local_sender, mut local_receiver) = mpsc::channel::<Payload>(1024);
        let local_sender = Arc::new(local_sender);
        let local_sender_clone = local_sender.clone();

        let payload = utils::convert(
            GrpcMessageBuilder::new(setup_request)
                .build()
                .into_payload(),
            "setup message convert to grpc message occur an error",
        )?;

        let send_ret = local_sender.send(payload).await;
        if let Err(e) = send_ret {
            error!("bi stream hash been closed. {}", e);
            return Err(ErrResult("bi stream hash been closed".to_string()));
        }

        let (notifier, waiter) = oneshot::channel::<()>();
        let local_stream = GrpcStream::<Payload>::new(Box::pin(stream! {
            // notify
            let _ = notifier.send(());
            debug!("open local stream.");
            while let Some(request) = local_receiver.recv().await {
                debug!("local stream send message to server");
                yield request
            }
            warn!("local stream closed!");
        }));

        let (cb, rx, mut tk) =
            utils::create_grpc_callback::<Result<GrpcStream<Result<Payload, Error>>, Error>>();
        let call = NacosGrpcCall::BIRequestService((local_stream, cb));
        executor::spawn(service.call(call).in_current_span());

        let (conn_id_sender, conn_id_receiver) = oneshot::channel::<String>();
        executor::spawn(
            async move {
                tk.want();
                let server_stream =
                    match utils::recv_response(rx.await, "server stream callback failed") {
                        Ok(Ok(stream)) => stream,
                        Ok(Err(e)) => {
                            error!("can't open server stream. {}", e);
                            warn!("server stream closed!");
                            return;
                        }
                        Err(_) => {
                            warn!("server stream closed!");
                            return;
                        }
                    };

                // receive conn_id
                let conn_id = match utils::recv_response(
                    conn_id_receiver.await,
                    "server stream has already opened, but cannot get connection id",
                ) {
                    Ok(id) => id,
                    Err(_) => return,
                };

                // create span
                let span = debug_span!("bi_stream", conn_id = conn_id);
                async {
                    let mut server_stream = Box::pin(server_stream);
                    while let Some(Ok(response)) = server_stream.next().await {
                        last_active.store(now_millis(), Ordering::Release);
                        debug!("server stream receive message from server");
                        let Some(metadata) = response.metadata.as_ref() else {
                            debug!("response payload type field is empty, skip.");
                            continue;
                        };
                        debug!("server stream handler: {}", metadata.r#type);
                        let handler = server_stream_handlers.get(&metadata.r#type).cloned();
                        let handler = handler.unwrap_or_else(|| Arc::new(DefaultHandler));
                        let Some(ret) = handler.request_reply(response).in_current_span().await
                        else {
                            debug!("handler no response, don't need to send to server. skip.");
                            continue;
                        };
                        let ret = local_sender_clone.send(ret).await;
                        if let Err(e) = ret {
                            error!("send grpc message to server occur an error, {}", e);
                            break;
                        }
                    }
                    warn!("server stream closed!");
                    let _ =
                        health.compare_exchange(true, false, Ordering::SeqCst, Ordering::Acquire);
                }
                .instrument(span)
                .await;
            }
            .in_current_span(),
        );

        let _ = waiter.await;
        Ok(conn_id_sender)
    }

    async fn request_once<Req>(
        service: &mut M::Service,
        request: Req,
        request_context: &str,
    ) -> Result<(oneshot::Receiver<Result<Payload, Error>>, want::Taker), Error>
    where
        Req: GrpcRequestMessage + 'static,
    {
        let request = utils::convert(
            GrpcMessageBuilder::new(request).build().into_payload(),
            request_context,
        )?;

        let (cb, rx, mut tk) = utils::create_grpc_callback::<Result<Payload, Error>>();
        let grpc_call = NacosGrpcCall::RequestService((request, cb));
        executor::spawn(service.call(grpc_call));

        tk.want();
        Ok((rx, tk))
    }

    async fn connection_health_check(service: &mut M::Service) -> Result<(), Error> {
        info!("connection health check");

        let (rx, _tk) = Self::request_once(
            service,
            HealthCheckRequest::default(),
            "health check request message convert to grpc message occur an error",
        )
        .await?;

        let response = match tokio::time::timeout(HEALTH_CHECK_REQUEST_TIMEOUT, rx).await {
            Err(_) => {
                warn!(
                    "connection health check timeout after {:?}",
                    HEALTH_CHECK_REQUEST_TIMEOUT
                );
                return Err(ErrResult(
                    "connection health check request timeout".to_string(),
                ));
            }
            Ok(resp) => utils::convert(
                utils::recv_response(resp, "grpc request callback failed")?,
                "connection health check failed",
            )?,
        };

        let response = GrpcMessage::<HealthCheckResponse>::from_payload(response);
        if let Err(e) = response {
            let err_msg = "connection health check failed, convert to grpc message failed";
            warn!(
                "{}. If the retry is successful, please ignore it: {}",
                err_msg, e
            );
            return Err(ErrResult(err_msg.to_string()));
        }

        Ok(())
    }

    async fn check_server(service: &mut M::Service) -> Result<String, Error> {
        info!("check server");

        let (rx, _tk) = Self::request_once(
            service,
            ServerCheckRequest::new(),
            "server check request message convert to grpc message occur an error",
        )
        .await?;

        let response = utils::convert(
            utils::recv_response(rx.await, "grpc request callback failed")?,
            "check server failed",
        )?;

        let response = utils::convert(
            GrpcMessage::<ServerCheckResponse>::from_payload(response),
            "check server failed convert to grpc message failed",
        )?;

        let connection_id = utils::unwrap_option(
            response.into_body().connection_id,
            "check server failed connection id is empty",
        )?;

        Ok(connection_id)
    }
}

pub(crate) enum State<F, S> {
    Idle,
    Connecting(Pin<Box<F>>),
    Initializing(Pin<Box<dyn Future<Output = Result<(S, String), Error>> + Send>>),
    Connected(S),
    Retry(Pin<Box<dyn Future<Output = ()> + Send>>),
}

enum RetryCause {
    Connect(Error),
    Init(Error),
    Unhealthy,
    NotReady(Error),
}

impl<M> Service<Payload> for NacosGrpcConnection<M>
where
    M: MakeService<(), NacosGrpcCall, MakeError = Error, Error = Error> + Send + 'static,
    M::Error: Send + 'static,
    M::Response: Send + 'static,
    M::Future: Send + 'static,
    M::Service: Send + 'static,
    <M::Service as Service<NacosGrpcCall>>::Future: Send + 'static,
{
    type Response = Payload;

    type Error = Error;

    type Future = Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        let _span_enter =
            debug_span!(parent: None, "grpc_connection", id = self.id.as_str()).entered();

        loop {
            if !self.is_initialized && self.retry_count >= MAX_INIT_RETRIES {
                error!(
                    "Connection initialization failed (attempts: {}). \
                     The Nacos server may be unreachable. \
                     Please check server address and network connectivity.",
                    MAX_INIT_RETRIES
                );
                // FIXME Tower Service 契约违反：poll_ready 返回 Ok(()) 但 call() 返回错误
                //  但当前先与 emergency_start 配合，允许应急启动。
                self.retry_count = 0;
                return Poll::Ready(Ok(()));
            }

            let retry = match self.state {
                State::Idle => {
                    info!("create new connection.");
                    let send_ret = self.connection_id_watcher.0.send(None);
                    if let Err(e) = send_ret {
                        // this never happen maybe.
                        warn!(
                            "connection id watch channel exception, send to receiver error: {}",
                            e
                        );
                    }
                    let mk_fut = self.mk_service.make_service(());
                    self.state = State::Connecting(Box::pin(mk_fut));
                    continue;
                }
                State::Connecting(ref mut fut) => match fut.as_mut().poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(ret)) => {
                        let init_future = Box::pin(NacosGrpcConnection::<M>::init_connection(
                            ret,
                            self.client_version.clone(),
                            self.namespace.clone(),
                            self.labels.clone(),
                            self.client_abilities.clone(),
                            self.handler_map.clone(),
                            self.health.clone(),
                            self.last_active.clone(),
                        ));
                        self.state = State::Initializing(init_future);
                        continue;
                    }
                    Poll::Ready(Err(e)) => Some(RetryCause::Connect(e)),
                },
                State::Initializing(ref mut init) => {
                    info!("the new connection is initializing.");

                    match init.as_mut().poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Ok((service, connection_id))) => {
                            info!(
                                "the new connection init completed. connection id :{}",
                                connection_id
                            );

                            let send_ret = self
                                .connection_id_watcher
                                .0
                                .send(Some(connection_id.clone()));
                            if let Err(e) = send_ret {
                                // this never happen maybe.
                                warn!(
                                    "connection id watch channel exception, send connection id:{} to receiver error: {}",
                                    connection_id, e
                                );
                            }

                            self.retry_count = 0;
                            self.health.store(true, Ordering::Release);
                            self.last_active.store(now_millis(), Ordering::Release);
                            self.is_initialized = true;
                            self.state = State::Connected(service);
                            self.connection_id = Some(connection_id);
                            continue;
                        }
                        Poll::Ready(Err(e)) => Some(RetryCause::Init(e)),
                    }
                }
                State::Connected(ref mut service) => match service.poll_ready(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(_)) => {
                        if self.health.load(Ordering::Acquire) {
                            return Poll::Ready(Ok(()));
                        }
                        Some(RetryCause::Unhealthy)
                    }
                    Poll::Ready(Err(e)) => {
                        self.health.store(false, Ordering::Release);
                        Some(RetryCause::NotReady(e))
                    }
                },
                State::Retry(ref mut sleep) => {
                    let ret = sleep.as_mut().poll(cx);
                    if ret == Poll::Pending {
                        return Poll::Pending;
                    }
                    self.state = State::Idle;
                    continue;
                }
            };

            if let Some(cause) = retry {
                self.enter_retry(cause);
                continue;
            }
        }
    }

    fn call(&mut self, req: Payload) -> Self::Future {
        let conn_id = self.connection_id.as_deref().unwrap_or("None");

        let _span_enter = debug_span!("grpc_connection", conn_id = conn_id).entered();

        match self.state {
            State::Connected(ref mut service) => {
                let (cb, rx, mut tk) = utils::create_grpc_callback::<Result<Payload, Error>>();
                let grpc_call = NacosGrpcCall::RequestService((req, cb));
                let call_task = service.call(grpc_call).in_current_span();
                let last_active = self.last_active.clone();
                let response_fut = async move {
                    tk.want();
                    let response = utils::recv_response(rx.await, "sender has been drop")?;
                    last_active.store(now_millis(), Ordering::Release);
                    response
                }
                .in_current_span();
                executor::spawn(call_task);
                Box::pin(response_fut)
            }
            _ => {
                let fut: Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send>> = Box::pin(
                    async move { Err(ErrResult("the connection is not connected".to_string())) }
                        .in_current_span(),
                );
                Box::pin(fut)
            }
        }
    }
}

pub(crate) struct FailoverConnection<S>
where
    S: Service<Payload, Error = Error, Response = Payload> + Send + 'static,
    S::Future: Send + 'static,
{
    id: String,
    inner: Buffer<Payload, S::Future>,
    svc_health: Arc<AtomicBool>,
    active_health_check: Arc<AtomicBool>,
}

impl<S> FailoverConnection<S>
where
    S: Service<Payload, Error = Error, Response = Payload> + Send + 'static,
    S::Future: Send + 'static,
{
    pub(crate) fn new(
        id: String,
        svc: S,
        svc_health: Arc<AtomicBool>,
        svc_last_active: Arc<AtomicU64>,
    ) -> Self {
        let (inner, work) = Buffer::pair(svc, 1024);
        executor::spawn(work);

        let active_health_check = Arc::new(AtomicBool::new(true));

        // start health check task
        executor::spawn(
            FailoverConnection::<S>::health_check(
                inner.clone(),
                active_health_check.clone(),
                svc_health.clone(),
                svc_last_active.clone(),
            )
            .instrument(debug_span!("health_check", id = id)),
        );

        Self {
            id,
            inner,
            svc_health,
            active_health_check,
        }
    }

    #[allow(dead_code)]
    #[instrument(fields(id = self.id), skip_all)]
    pub(crate) fn failover(&self) {
        let _ = self
            .svc_health
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::Acquire);
    }

    async fn health_check(
        mut svc: Buffer<Payload, S::Future>,
        active_health_check: Arc<AtomicBool>,
        svc_health: Arc<AtomicBool>,
        last_active: Arc<AtomicU64>,
    ) {
        while active_health_check.load(Ordering::Acquire) {
            debug!("health check.");

            // only probe when the connection has been idle for a full interval
            let idle_millis = now_millis().saturating_sub(last_active.load(Ordering::Acquire));
            if idle_millis < HEALTH_CHECK_INTERVAL.as_millis() as u64 {
                sleep(HEALTH_CHECK_INTERVAL).await;
                continue;
            }

            let mut healthy = false;
            for attempt in 0..HEALTH_CHECK_RETRY_TIMES {
                if attempt > 0 {
                    let jitter = Duration::from_millis(rand::random_range(..500_u64));
                    sleep(jitter).await;
                }

                let Ok(health_check_request) =
                    GrpcMessageBuilder::new(HealthCheckRequest::default())
                        .build()
                        .into_payload()
                else {
                    error!("health check failed, grpc message can not convert to payload. retry.");
                    break;
                };

                let ready = futures_util::future::poll_fn(|cx| svc.poll_ready(cx))
                    .in_current_span()
                    .await;
                if ready.is_err() {
                    warn!("connection not ready, wait.");
                    break;
                }

                let call_result = tokio::time::timeout(
                    HEALTH_CHECK_REQUEST_TIMEOUT,
                    svc.call(health_check_request).in_current_span(),
                )
                .await;

                match call_result {
                    Err(_) => {
                        error!(
                            "health check failed, request timeout after {:?}, retry.",
                            HEALTH_CHECK_REQUEST_TIMEOUT
                        );
                    }
                    Ok(Err(e)) => {
                        error!("health check failed, send health check request failed, retry. {e}");
                    }
                    Ok(Ok(response)) => {
                        if GrpcMessage::<HealthCheckResponse>::from_payload(response).is_ok() {
                            healthy = true;
                            break;
                        }
                        error!("health check failed, error response, retry.");
                    }
                }
            }

            if healthy {
                last_active.store(now_millis(), Ordering::Release);
            } else {
                let _ =
                    svc_health.compare_exchange(true, false, Ordering::SeqCst, Ordering::Acquire);
            }

            sleep(HEALTH_CHECK_INTERVAL).await;
        }

        warn!("stop health check task.");
    }
}

impl<S> Drop for FailoverConnection<S>
where
    S: Service<Payload, Error = Error, Response = Payload> + Send + 'static,
    S::Future: Send + 'static,
{
    fn drop(&mut self) {
        self.active_health_check.store(false, Ordering::Release);
    }
}

#[allow(clippy::disallowed_methods)] // Tests mock! has std::result::Result::unwrap
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub(crate) trait SendRequest: Send {
    async fn send_request(&self, request: Payload) -> Result<Payload, Error>;
}

#[async_trait]
impl<S> SendRequest for FailoverConnection<S>
where
    S: Service<Payload, Error = Error, Response = Payload> + Send + 'static,
    S::Future: Send + 'static,
{
    #[instrument(fields(id = self.id), skip_all)]
    async fn send_request(&self, request: Payload) -> Result<Payload, Error> {
        let mut svc = self.inner.clone();
        let _ = future::poll_fn(|cx| svc.poll_ready(cx))
            .in_current_span()
            .await?;
        let ret = svc.call(request).in_current_span().await;
        ret.map_err(GrpcBufferRequest)
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // Tests mock! has std::result::Result::unwrap
pub mod nacos_grpc_connection_tests {

    use super::*;
    use crate::common::remote::grpc::tonic::GrpcCallTask;
    use mockall::*;
    use std::task::Context;

    mock! {
        Tonic{}

        impl Service<NacosGrpcCall> for Tonic {

            type Response = ();

            type Error = Error;

            type Future = GrpcCallTask;

            fn poll_ready<'a>(&mut self, cx: &mut Context<'a>) -> Poll<Result<(), <Self as Service<NacosGrpcCall>>::Error>>;

            fn call(&mut self, call: NacosGrpcCall) -> <Self as Service<NacosGrpcCall>>::Future;


        }
    }

    mock! {

        TonicBuilder{}
        impl Service<()> for TonicBuilder{
            type Response = MockTonic;

            type Error = Error;

            type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

            fn poll_ready<'a>(&mut self, cx: &mut Context<'a>) -> Poll<Result<(), <Self as Service<()>>::Error>>;

            fn call(&mut self, request: ()) -> <Self as Service<()>>::Future;

        }
    }
}
