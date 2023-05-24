use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;
use std::time::Duration;
use std::{collections::HashMap, pin::Pin, sync::Arc};

use async_stream::stream;
use futures::StreamExt;
use futures::{future, Future};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::sleep;
use tonic::async_trait;
use tower::buffer::Buffer;
use tower::{MakeService, Service};
use tracing::{debug, debug_span, error, info, instrument, warn, Instrument};

use crate::api::error::Error::ErrResult;
use crate::api::error::Error::GrpcBufferRequest;
use crate::common::executor;
use crate::common::remote::grpc::handlers::default_handler::DefaultHandler;
use crate::common::remote::grpc::message::request::{
    ConnectionSetupRequest, HealthCheckRequest, NacosClientAbilities, ServerCheckRequest,
};
use crate::common::remote::grpc::message::response::{HealthCheckResponse, ServerCheckResponse};
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};
use crate::common::remote::grpc::nacos_grpc_service::GrpcStream;
use crate::{
    api::error::Error, common::remote::grpc::nacos_grpc_service::Callback, nacos_proto::v2::Payload,
};

use super::nacos_grpc_service::NacosGrpcCall;
use super::nacos_grpc_service::ServerRequestHandler;

type ConnectedListener = Arc<dyn Fn(String) + Send + Sync + 'static>;
type DisconnectedListener = Arc<dyn Fn(String) + Send + Sync + 'static>;

type HandlerMap = HashMap<String, Arc<dyn ServerRequestHandler>>;
const MAX_RETRY: i32 = 6;

fn sleep_time(retry_count: i32) -> i32 {
    if retry_count > MAX_RETRY {
        1 << (retry_count % MAX_RETRY)
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
    connection_id: Option<String>,
    retry_count: i32,
    connection_id_watcher: (
        watch::Sender<Option<String>>,
        watch::Receiver<Option<String>>,
    ),
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
            connection_id: None,
            retry_count: 0,
            connection_id_watcher,
        }
    }

    pub(crate) fn connected_listener(self, listener: ConnectedListener) -> Self {
        let mut rx = self.connection_id_watcher.1.clone();
        let watch_fu = async move {
            let mut previous_id = None;
            while rx.changed().await.is_ok() {
                let current_id = { rx.borrow().clone() };

                // if previous id is none and the current id is some then the state is connected
                if previous_id.is_none() && current_id.is_some() {
                    let current_id = current_id.as_ref().unwrap().clone();
                    listener(current_id);
                }
                previous_id = current_id;
            }
            debug!("connected listener quit.");
        };
        executor::spawn(watch_fu);
        self
    }

    pub(crate) fn disconnected_listener(self, listener: DisconnectedListener) -> Self {
        let mut rx = self.connection_id_watcher.1.clone();
        let watch_fu = async move {
            let mut previous_id: Option<String> = None;
            while rx.changed().await.is_ok() {
                let current_id = { rx.borrow().clone() };

                // if previous id is some and the current id is none then the state is disconnected
                if previous_id.is_some() && current_id.is_none() {
                    let previous_id = previous_id.as_ref().unwrap().clone();
                    listener(previous_id);
                }
                previous_id = current_id;
            }
            debug!("disconnect listener quit.");
        };
        executor::spawn(watch_fu);
        self
    }

    pub(crate) fn into_failover_connection(
        self,
        id: String,
    ) -> FailoverConnection<NacosGrpcConnection<M>> {
        let svc_health = self.health.clone();
        FailoverConnection::new(id, self, svc_health)
    }

    async fn init_connection(
        mut service: M::Service,
        client_version: String,
        namespace: String,
        labels: HashMap<String, String>,
        client_abilities: NacosClientAbilities,
        handler_map: Arc<HandlerMap>,
        health: Arc<AtomicBool>,
    ) -> Result<(M::Service, String), Error> {
        // setup
        let conn_id_sender = NacosGrpcConnection::<M>::setup(
            handler_map,
            &mut service,
            health,
            client_version,
            namespace,
            labels,
            client_abilities,
        )
        .in_current_span()
        .await?;

        // connection health check
        for _ in [(); 10] {
            let health_check = NacosGrpcConnection::<M>::connection_health_check(&mut service)
                .in_current_span()
                .await;
            if health_check.is_err() {
                sleep(Duration::from_millis(300)).await;
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
            error!("send connection id to bi stream task occur an error. please check connection state. {e}");
        }

        // set connection id
        Ok((service, connection_id))
    }

    async fn setup(
        server_stream_handlers: Arc<HandlerMap>,
        service: &mut M::Service,
        health: Arc<AtomicBool>,
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

        let payload = GrpcMessageBuilder::new(setup_request)
            .build()
            .into_payload();

        if let Err(e) = payload {
            // grpc message convert failed, should panic.
            error!(
                "setup message convert to grpc message occur an error. {}",
                e
            );
            return Err(ErrResult(
                "setup message convert to grpc message occur an error".to_string(),
            ));
        }
        let payload = payload.unwrap();

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

        let (gv, mut tk) = want::new();

        let (tx, rx) = oneshot::channel::<Result<GrpcStream<Result<Payload, Error>>, Error>>();

        let call_back = Callback::new(gv, tx);
        let call = NacosGrpcCall::BIRequestService((local_stream, call_back));
        executor::spawn(service.call(call).in_current_span());

        let (conn_id_sender, conn_id_receiver) = oneshot::channel::<String>();
        executor::spawn(async move {
            tk.want();
            let server_stream = rx.await;
            if let Err(e) = server_stream {
                error!("server stream callback failed. {}", e);
                warn!("server stream closed!");
                return;
            }

            let server_stream = server_stream.unwrap();
            if let Err(e) = server_stream {
                error!("can't open server stream. {}", e);
                warn!("server stream closed!");
                return;
            }

            let server_stream = server_stream.unwrap();

            // receive conn_id
            let conn_id = conn_id_receiver.await;
            if let Err(e) = conn_id {
                // receive conn_id error? impossible unless the sender has already dropped.
                error!("the server stream has already opened, but cannot get connection id! quit server stream task. {e}");
                return;
            }
            let conn_id = conn_id.unwrap();

            // create span
            let span = debug_span!("bi_stream", conn_id = conn_id);
            async  {
                let mut server_stream = Box::pin(server_stream);
                while let Some(Ok(response)) = server_stream.next().await {
                    debug!("server stream receive message from server");
                    let handler_key = response
                        .metadata
                        .as_ref()
                        .map(|meta_data| meta_data.r#type.clone());
                    if handler_key.is_none() {
                        debug!("response payload type field is empty, skip.");
                        continue;
                    }
                    let handler_key = handler_key.unwrap();
                    debug!("server stream handler: {}", handler_key);
                    let handler = server_stream_handlers.get(&handler_key).cloned();
                    let handler = handler.unwrap_or_else(|| Arc::new(DefaultHandler));
                    let ret = handler.request_reply(response).in_current_span().await;
                    if ret.is_none() {
                        debug!(
                            "handler no response, don't need to send to server. skip. key:{}",
                            handler_key
                        );
                        continue;
                    }
                    let ret = ret.unwrap();
                    let ret = local_sender_clone.send(ret).await;
                    if let Err(e) = ret {
                        error!("send grpc message to server occur an error, {}", e);
                        break;
                    }
                }
                warn!("server stream closed!");
                health.store(false, Ordering::Release);
            }.instrument(span).await;
        }.in_current_span());

        let _ = waiter.await;
        Ok(conn_id_sender)
    }

    async fn connection_health_check(service: &mut M::Service) -> Result<(), Error> {
        info!("connection health check");

        let request = HealthCheckRequest::default();
        let request = GrpcMessageBuilder::new(request).build().into_payload();
        if let Err(e) = request {
            error!(
                "health check request message convert to grpc message occur an error. {}",
                e
            );
            return Err(ErrResult(
                "health check request message convert to grpc message occur an error".to_string(),
            ));
        }
        let request = request.unwrap();

        let (gv, mut tk) = want::new();

        let (tx, rx) = oneshot::channel::<Result<Payload, Error>>();

        let call_back = Callback::new(gv, tx);
        let grpc_call = NacosGrpcCall::RequestService((request, call_back));

        executor::spawn(service.call(grpc_call));

        tk.want();
        let response = rx.await;
        if let Err(e) = response {
            error!("grpc request callback failed. {}", e);
            return Err(ErrResult("grpc request callback failed".to_string()));
        }
        let response = response.unwrap();
        if let Err(e) = response {
            error!("connection health check failed: {}", e);
            return Err(ErrResult("connection health check failed".to_string()));
        }
        let response = response.unwrap();

        let response = GrpcMessage::<HealthCheckResponse>::from_payload(response);
        if let Err(e) = response {
            error!(
                "connection health check failed convert to grpc message failed. {}",
                e
            );
            return Err(ErrResult(
                "connection health check failed convert to grpc message failed".to_string(),
            ));
        }
        Ok(())
    }

    async fn check_server(service: &mut M::Service) -> Result<String, Error> {
        info!("check server");

        let request = ServerCheckRequest::new();
        let request = GrpcMessageBuilder::new(request).build().into_payload();
        if let Err(e) = request {
            error!(
                "server check request message convert to grpc message occur an error. {}",
                e
            );
            return Err(ErrResult(
                "server check request message convert to grpc message occur an error".to_string(),
            ));
        }
        let request = request.unwrap();

        let (gv, mut tk) = want::new();

        let (tx, rx) = oneshot::channel::<Result<Payload, Error>>();

        let call_back = Callback::new(gv, tx);
        let grpc_call = NacosGrpcCall::RequestService((request, call_back));

        executor::spawn(service.call(grpc_call));

        tk.want();
        let response = rx.await;
        if let Err(e) = response {
            error!("grpc request callback failed. {}", e);
            return Err(ErrResult("grpc request callback failed".to_string()));
        }
        let response = response.unwrap();
        if let Err(e) = response {
            error!("check server failed: {}", e);
            return Err(ErrResult("check server failed".to_string()));
        }
        let response = response.unwrap();

        let response = GrpcMessage::<ServerCheckResponse>::from_payload(response);
        if let Err(e) = response {
            error!("check server failed convert to grpc message failed. {}", e);
            return Err(ErrResult(
                "check server failed convert to grpc message failed".to_string(),
            ));
        }

        let response = response.unwrap();

        let response = response.into_body();
        let connection_id = response.connection_id;

        if connection_id.is_none() {
            error!("check server failed connection id is empty");
            return Err(ErrResult(
                "check server failed connection id is empty".to_string(),
            ));
        }

        let connection_id = connection_id.unwrap();

        Ok(connection_id)
    }
}

pub(crate) enum State<F, S> {
    Idle,
    Connecting(F),
    Initializing(Box<dyn Future<Output = Result<(S, String), Error>> + Send>),
    Connected(S),
    Retry(Box<dyn Future<Output = ()> + Send>),
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

    type Future = ResponseFuture;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        let _span_enter =
            debug_span!(parent: None, "grpc_connection", id = self.id.clone()).entered();

        loop {
            match self.state {
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
                    self.state = State::Connecting(mk_fut);
                    continue;
                }
                State::Connecting(ref mut fut) => {
                    let pin = unsafe { Pin::new_unchecked(fut) };
                    let ret = pin.poll(cx);
                    match ret {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Ok(ret)) => {
                            let init_future = Box::new(NacosGrpcConnection::<M>::init_connection(
                                ret,
                                self.client_version.clone(),
                                self.namespace.clone(),
                                self.labels.clone(),
                                self.client_abilities.clone(),
                                self.handler_map.clone(),
                                self.health.clone(),
                            ));
                            self.state = State::Initializing(init_future);
                            continue;
                        }
                        Poll::Ready(Err(e)) => {
                            self.retry_count += 1;
                            let sleep_time = sleep_time(self.retry_count);
                            error!("create connection error, this operate will be retry after {} sec, retry count:{}. {}", sleep_time,  self.retry_count, e);
                            self.state = State::Retry(Box::new(sleep(Duration::from_secs(
                                sleep_time as u64,
                            ))));
                            continue;
                        }
                    }
                }
                State::Initializing(ref mut init) => {
                    info!("the new connection is initializing.");
                    let init = init.as_mut();

                    let init = unsafe { Pin::new_unchecked(init) };
                    let ret = init.poll(cx);
                    match ret {
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
                                warn!("connection id watch channel exception, send connection id:{} to receiver error: {}", connection_id, e);
                            }

                            self.retry_count = 0;
                            self.health.store(true, Ordering::Release);
                            self.state = State::Connected(service);
                            self.connection_id = Some(connection_id);
                            continue;
                        }
                        Poll::Ready(Err(e)) => {
                            self.retry_count += 1;
                            let sleep_time = sleep_time(self.retry_count);
                            error!("initializing connection error, this operate will be retry after {} sec, retry count:{}. {}", sleep_time,  self.retry_count, e);
                            self.state = State::Retry(Box::new(sleep(Duration::from_secs(
                                sleep_time as u64,
                            ))));
                            continue;
                        }
                    }
                }
                State::Connected(ref mut service) => {
                    let ready = service.poll_ready(cx);
                    match ready {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Ok(_)) => return Poll::Ready(Ok(())),
                        Poll::Ready(Err(e)) => {
                            self.health.store(false, Ordering::Release);
                            self.retry_count += 1;
                            let sleep_time = sleep_time(self.retry_count);
                            error!("connection {:?} not ready, destroy connection and retry, this operate will be retry after {} sec, retry count:{}. {}", self.connection_id,  sleep_time,  self.retry_count, e);
                            self.state = State::Retry(Box::new(sleep(Duration::from_secs(
                                sleep_time as u64,
                            ))));
                            continue;
                        }
                    }
                }

                State::Retry(ref mut sleep) => {
                    let sleep = sleep.as_mut();

                    let sleep = unsafe { Pin::new_unchecked(sleep) };
                    let ret = sleep.poll(cx);
                    if ret == Poll::Pending {
                        return Poll::Pending;
                    }
                    self.state = State::Idle;
                    continue;
                }
            }
        }
    }

    fn call(&mut self, req: Payload) -> Self::Future {
        let conn_id = if let Some(ref conn_id) = self.connection_id {
            conn_id.clone()
        } else {
            "None".to_string()
        };

        let _span_enter = debug_span!("grpc_connection", conn_id = conn_id).entered();

        if !self.health.load(Ordering::Acquire) {
            self.state = State::Idle;
            return ResponseFuture::new(Box::new(
                async move { Err(ErrResult("the connection is not in health".to_string())) }
                    .in_current_span(),
            ));
        }

        match self.state {
            State::Connected(ref mut service) => {
                let (gv, mut tk) = want::new();
                let (tx, rx) = oneshot::channel::<Result<Payload, Error>>();
                let call_back = Callback::new(gv, tx);
                let grpc_call = NacosGrpcCall::RequestService((req, call_back));
                let call_task = service.call(grpc_call).in_current_span();
                let response_fut = async move {
                    tk.want();
                    let response = rx.await;
                    if response.is_err() {
                        return Err(ErrResult("sender has been drop".to_string()));
                    }
                    response.unwrap()
                }
                .in_current_span();
                executor::spawn(call_task);
                ResponseFuture::new(Box::new(response_fut))
            }
            _ => ResponseFuture::new(Box::new(
                async move { Err(ErrResult("the connection is not connected".to_string())) }
                    .in_current_span(),
            )),
        }
    }
}

pub(crate) struct ResponseFuture {
    inner: Box<dyn Future<Output = Result<Payload, Error>> + Send>,
}

impl ResponseFuture {
    pub(crate) fn new(inner: Box<dyn Future<Output = Result<Payload, Error>> + Send>) -> Self {
        Self { inner }
    }
}

impl Future for ResponseFuture {
    type Output = Result<Payload, Error>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let pin = unsafe { Pin::new_unchecked(self.get_mut().inner.as_mut()) };
        pin.poll(cx)
    }
}

pub(crate) struct FailoverConnection<S>
where
    S: Service<Payload, Error = Error, Response = Payload> + Send + 'static,
    S::Future: Send + 'static,
{
    id: String,
    inner: Buffer<S, Payload>,
    svc_health: Arc<AtomicBool>,
    active_health_check: Arc<AtomicBool>,
}

impl<S> FailoverConnection<S>
where
    S: Service<Payload, Error = Error, Response = Payload> + Send + 'static,
    S::Future: Send + 'static,
{
    pub(crate) fn new(id: String, svc: S, svc_health: Arc<AtomicBool>) -> Self {
        let (inner, work) = Buffer::pair(svc, 1024);
        executor::spawn(work);

        let active_health_check = Arc::new(AtomicBool::new(true));

        // start health check task
        executor::spawn(
            FailoverConnection::<S>::health_check(
                inner.clone(),
                active_health_check.clone(),
                svc_health.clone(),
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

    #[instrument(fields(id = self.id), skip_all)]
    pub(crate) fn failover(&self) {
        self.svc_health.store(false, Ordering::Release);
    }

    async fn health_check(
        mut svc: Buffer<S, Payload>,
        active_health_check: Arc<AtomicBool>,
        svc_health: Arc<AtomicBool>,
    ) {
        while active_health_check.load(Ordering::Acquire) {
            debug!("health check.");
            let health_check_request = HealthCheckRequest::default();
            let health_check_request = GrpcMessageBuilder::new(health_check_request)
                .build()
                .into_payload();
            if let Err(e) = health_check_request {
                // should panic
                error!(
                    "health check failed, grpc message can not convert to payload. retry. {}",
                    e
                );
                sleep(Duration::from_secs(5)).await;
                continue;
            }
            let health_check_request = health_check_request.unwrap();
            let ready = futures_util::future::poll_fn(|cx| svc.poll_ready(cx))
                .in_current_span()
                .await;
            if ready.is_err() {
                warn!("connection not ready, wait.");
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            let response = svc.call(health_check_request).in_current_span().await;
            if let Err(e) = response {
                svc_health.store(false, Ordering::Release);
                error!("health check failed, retry. {}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            let response = response.unwrap();
            let response = GrpcMessage::<HealthCheckResponse>::from_payload(response);
            if let Err(e) = response {
                svc_health.store(false, Ordering::Release);
                error!("health check failed, grpc message can not convert to HealthCheckResponse, retry. {}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            sleep(Duration::from_secs(5)).await;
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
        ret.map_err(|error| GrpcBufferRequest(error))
    }
}

#[cfg(test)]
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
