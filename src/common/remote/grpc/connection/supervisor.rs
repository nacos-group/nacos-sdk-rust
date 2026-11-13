use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::error;

use crate::api::error::Error;
use crate::nacos_proto::v2::Payload;

use super::config::ConnectionSettings;
use super::handshake;
use super::health;
use super::{ConnectionEvent, Connector, HandlerMap, Spawner, Transport, now_millis};

#[derive(Debug)]
pub(crate) struct OutboundRequest {
    pub(crate) payload: Payload,
    pub(crate) reply: oneshot::Sender<Result<Payload, Error>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    Connecting,
    Negotiating,
    Ready { connection_id: String },
    Backoff { attempt: u32 },
    Stopped,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum StartupMode {
    WaitForReady { max_attempts: u32 },
    Background,
}

pub(crate) struct StartupWaiter(pub(crate) Option<oneshot::Receiver<Result<(), Error>>>);

impl StartupWaiter {
    pub(crate) async fn wait(self) -> Result<(), Error> {
        match self.0 {
            None => Ok(()),
            Some(receiver) => receiver.await.map_err(|_| {
                Error::ErrResult("connection supervisor stopped during startup".to_string())
            })?,
        }
    }
}

struct ActiveConnection {
    transport: Arc<dyn Transport>,
    connection_id: String,
    pump: JoinHandle<()>,
    health: JoinHandle<()>,
    events_rx: mpsc::Receiver<ConnectionEvent>,
    last_active: Arc<AtomicU64>,
    cancel: watch::Sender<bool>,
}

impl ActiveConnection {
    fn abort(&self) {
        self.pump.abort();
        self.health.abort();
        let _ = self.cancel.send_replace(true);
    }
}

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        // `JoinHandle::drop` only detaches, so every path that loses an active
        // connection (including a `select!` racing `connect_once` against
        // shutdown) must tear the pump and health task down here.
        self.abort();
    }
}

pub(crate) struct Supervisor<C: Connector> {
    pub(crate) id: String,
    pub(crate) connector: C,
    pub(crate) handler_map: Arc<HandlerMap>,
    pub(crate) settings: ConnectionSettings,
    pub(crate) rx: mpsc::Receiver<OutboundRequest>,
    pub(crate) state_tx: watch::Sender<ConnectionState>,
    pub(crate) connection_tx: watch::Sender<Option<String>>,
    pub(crate) startup_tx: Option<oneshot::Sender<Result<(), Error>>>,
    pub(crate) startup_mode: StartupMode,
    pub(crate) spawner: Arc<dyn Spawner>,
    pub(crate) server_abilities: Arc<RwLock<HashMap<String, bool>>>,
}

impl<C: Connector> Supervisor<C> {
    async fn shutdown_requested(rx: &mut watch::Receiver<bool>) -> bool {
        if *rx.borrow() {
            return true;
        }
        rx.changed().await.is_err() || *rx.borrow()
    }

    fn publish(&self, state: ConnectionState) {
        let _ = self.state_tx.send(state);
    }

    async fn connect_once(
        &mut self,
        shutdown: watch::Receiver<bool>,
    ) -> Result<ActiveConnection, Error> {
        let timeout = self.settings.request_timeout;
        let transport = tokio::time::timeout(timeout, self.connector.connect())
            .await
            .map_err(|_| Error::ErrResult("connect timeout".to_string()))??;
        self.publish(ConnectionState::Negotiating);
        let last_active = Arc::new(AtomicU64::new(now_millis()));
        let (events_tx, events_rx) = mpsc::channel(8);
        let negotiated = handshake::negotiate(
            transport,
            &self.settings,
            self.handler_map.clone(),
            events_tx.clone(),
            last_active.clone(),
            self.spawner.as_ref(),
        )
        .await?;
        *self
            .server_abilities
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = negotiated.abilities.clone();
        let (cancel, _) = watch::channel(false);
        let health = self.spawner.spawn(Box::pin(health::run(
            negotiated.transport.clone(),
            last_active.clone(),
            events_tx,
            shutdown,
            self.settings.clone(),
        )));
        Ok(ActiveConnection {
            transport: negotiated.transport,
            connection_id: negotiated.connection_id,
            pump: negotiated.pump,
            health,
            events_rx,
            last_active,
            cancel,
        })
    }

    async fn serve(
        &mut self,
        mut active: ActiveConnection,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<(), Error> {
        loop {
            tokio::select! {
                biased;
                _ = Self::shutdown_requested(shutdown) => {
                    active.abort();
                    return Ok(());
                }
                event = active.events_rx.recv() => {
                    active.abort();
                    return Err(match event {
                        Some(ConnectionEvent::StreamClosed(error))
                        | Some(ConnectionEvent::Unhealthy(error)) => error,
                        None => Error::ErrResult("connection event channel closed".to_string()),
                    });
                }
                request = self.rx.recv() => {
                    let Some(OutboundRequest {
                        payload,
                        mut reply,
                    }) = request
                    else {
                        active.abort();
                        return Ok(());
                    };
                    if reply.is_closed() {
                        continue;
                    }
                    let transport = active.transport.clone();
                    let last_active = active.last_active.clone();
                    let mut cancel = active.cancel.subscribe();
                    self.spawner.spawn(Box::pin(async move {
                        let result = tokio::select! {
                            result = transport.unary(payload) => result,
                            _ = cancel.changed() => Err(Error::ClientUnhealthy("connection lost".to_string())),
                            // The caller gave up (deadline or dropped future): drop
                            // the transport call instead of detaching it. The
                            // receiver is gone, so returning directly skips the
                            // unobservable error value.
                            _ = reply.closed() => return,
                        };
                        if result.is_ok() {
                            last_active.store(now_millis(), Ordering::Release);
                        }
                        let _ = reply.send(result);
                    }));
                }
            }
        }
    }

    fn fail_queued(&mut self) -> bool {
        loop {
            match self.rx.try_recv() {
                Ok(request) => {
                    let _ = request.reply.send(Err(Error::ClientUnhealthy(
                        "connection is not ready".to_string(),
                    )));
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => return true,
            }
        }
    }

    pub(crate) async fn run(mut self, mut shutdown_rx: watch::Receiver<bool>) {
        let mut retry_attempt = 0u32;
        let mut startup_attempts = 0u32;
        let mut ever_ready = false;

        loop {
            if *shutdown_rx.borrow() {
                break;
            }
            self.publish(ConnectionState::Connecting);

            let connected = tokio::select! {
                result = self.connect_once(shutdown_rx.clone()) => Some(result),
                _ = Self::shutdown_requested(&mut shutdown_rx) => None,
            };
            let Some(connected) = connected else {
                break;
            };

            let mut retry_delay = None;
            match connected {
                Ok(active) => {
                    ever_ready = true;
                    let ready_at = tokio::time::Instant::now();
                    let connection_id = active.connection_id.clone();
                    let _ = self.connection_tx.send(Some(connection_id.clone()));
                    self.publish(ConnectionState::Ready { connection_id });
                    if let Some(tx) = self.startup_tx.take() {
                        let _ = tx.send(Ok(()));
                    }

                    let result = self.serve(active, &mut shutdown_rx).await;
                    let _ = self.connection_tx.send(None);
                    self.publish(ConnectionState::Connecting);

                    if ready_at.elapsed() >= self.settings.retry.stable_reset_after() {
                        retry_attempt = 0;
                    }
                    match result {
                        Ok(()) => break,
                        Err(error) => {
                            retry_attempt = retry_attempt.saturating_add(1);
                            error!(
                                "connection lost (attempt {}), retry after backoff. {}",
                                retry_attempt, error
                            );
                        }
                    }
                }
                Err(error) => {
                    // An unsupported server cannot become compatible by
                    // retrying, so a startup attempt fails immediately with that
                    // error. Once the connection has been ready at least once the
                    // server may have been replaced, so it is logged and retried.
                    if !ever_ready && matches!(error, Error::UnsupportedServerVersion(_)) {
                        self.publish(ConnectionState::Stopped);
                        if let Some(tx) = self.startup_tx.take() {
                            let _ = tx.send(Err(error));
                        }
                        let _ = self.fail_queued();
                        return;
                    }
                    if !ever_ready {
                        startup_attempts = startup_attempts.saturating_add(1);
                        if let StartupMode::WaitForReady { max_attempts } = self.startup_mode
                            && startup_attempts >= max_attempts.max(1)
                        {
                            self.publish(ConnectionState::Stopped);
                            if let Some(tx) = self.startup_tx.take() {
                                let _ = tx.send(Err(error));
                            }
                            let _ = self.fail_queued();
                            return;
                        }
                    }
                    retry_attempt = retry_attempt.saturating_add(1);
                    let delay = self.settings.retry.delay(retry_attempt.saturating_sub(1));
                    error!(
                        "connect failed (attempt {}, retry in {:?}), server: {}, cause: {}",
                        retry_attempt, delay, self.id, error
                    );
                    retry_delay = Some(delay);
                }
            }

            if self.fail_queued() {
                break;
            }

            let delay = retry_delay
                .unwrap_or_else(|| self.settings.retry.delay(retry_attempt.saturating_sub(1)));
            self.publish(ConnectionState::Backoff {
                attempt: retry_attempt,
            });
            // The deadline is anchored once so failing queued requests does not
            // shorten the backoff and trigger an immediate reconnect storm.
            let retry_deadline = tokio::time::Instant::now() + delay;
            let exit = loop {
                tokio::select! {
                    _ = tokio::time::sleep_until(retry_deadline) => break false,
                    _ = Self::shutdown_requested(&mut shutdown_rx) => break true,
                    request = self.rx.recv() => {
                        match request {
                            Some(request) => {
                                let _ = request.reply.send(Err(Error::ClientUnhealthy(
                                    "connection is not ready".to_string(),
                                )));
                            }
                            None => break true,
                        }
                    }
                }
            };
            if exit {
                break;
            }
        }

        self.publish(ConnectionState::Stopped);
        let _ = self.fail_queued();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use super::super::ConnectionHandle;
    use super::super::SendRequest;
    use super::*;
    use crate::common::remote::grpc::connection::config::ConnectionSettings;
    use crate::common::remote::grpc::connection::{HandlerMap, Spawner, Transport};
    use crate::common::remote::grpc::message::GrpcMessageBuilder;
    use crate::common::remote::grpc::message::request::SetupAckRequest;
    use crate::common::remote::grpc::message::response::ServerCheckResponse;
    use crate::common::remote::grpc::nacos_grpc_service::GrpcStream;

    #[derive(Clone)]
    struct ScriptedConnector {
        results: Arc<Mutex<VecDeque<Result<Arc<dyn Transport>, Error>>>>,
        connects: Arc<AtomicUsize>,
    }

    impl ScriptedConnector {
        fn once(transport: Arc<dyn Transport>) -> Self {
            Self::with_transports(vec![transport])
        }

        /// Connects to `transports` in order, returning `Err` for every attempt
        /// past the last scripted transport.
        fn with_transports(transports: Vec<Arc<dyn Transport>>) -> Self {
            Self::from_results(transports.into_iter().map(Ok).collect())
        }

        fn from_results(results: Vec<Result<Arc<dyn Transport>, Error>>) -> Self {
            Self {
                results: Arc::new(Mutex::new(results.into())),
                connects: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn failing() -> Self {
            Self::from_results(Vec::new())
        }
    }

    #[async_trait]
    impl Connector for ScriptedConnector {
        async fn connect(&mut self) -> Result<Arc<dyn Transport>, Error> {
            self.connects.fetch_add(1, Ordering::SeqCst);
            self.results
                .lock()
                .expect("connector lock")
                .pop_front()
                .unwrap_or_else(|| Err(Error::ErrResult("connect failed".to_string())))
        }
    }

    /// Connector whose `connect()` never completes, so only shutdown can end
    /// the startup attempt.
    struct PendingConnector;

    #[async_trait]
    impl Connector for PendingConnector {
        async fn connect(&mut self) -> Result<Arc<dyn Transport>, Error> {
            std::future::pending().await
        }
    }

    struct ScriptedTransport {
        unary: Mutex<VecDeque<Result<Payload, Error>>>,
        ack: Payload,
        streams: Mutex<Vec<mpsc::UnboundedSender<Result<Payload, Error>>>>,
        pending_calls: AtomicUsize,
        abandoned_calls: AtomicBool,
    }

    /// Flags the moment the pending `unary` future is dropped, which is how a
    /// cancelled caller becomes observable from the transport side.
    struct AbandonedOnDrop<'a>(&'a AtomicBool);

    impl Drop for AbandonedOnDrop<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    impl ScriptedTransport {
        fn new(server_check: Payload) -> Self {
            let ack = SetupAckRequest {
                ability_table: Some(HashMap::from([(
                    "supportPersistentInstanceByGrpc".to_string(),
                    true,
                )])),
                ..Default::default()
            };
            Self {
                unary: Mutex::new(VecDeque::from([Ok(server_check)])),
                ack: GrpcMessageBuilder::new(ack)
                    .build()
                    .into_payload()
                    .expect("ack payload"),
                streams: Mutex::new(Vec::new()),
                pending_calls: AtomicUsize::new(0),
                abandoned_calls: AtomicBool::new(false),
            }
        }

        fn push_unary(&self, response: Result<Payload, Error>) {
            self.unary.lock().expect("unary lock").push_back(response);
        }

        fn close_stream(&self) {
            let sender = self.streams.lock().expect("streams lock").pop();
            drop(sender);
        }
    }

    #[async_trait]
    impl Transport for ScriptedTransport {
        async fn unary(&self, _payload: Payload) -> Result<Payload, Error> {
            let next = self.unary.lock().expect("unary lock").pop_front();
            match next {
                Some(result) => result,
                None => {
                    self.pending_calls.fetch_add(1, Ordering::SeqCst);
                    let _abandoned = AbandonedOnDrop(&self.abandoned_calls);
                    futures::future::pending::<Result<Payload, Error>>().await
                }
            }
        }

        async fn open_stream(
            &self,
            _request: GrpcStream<Payload>,
        ) -> Result<GrpcStream<Result<Payload, Error>>, Error> {
            let (tx, mut rx) = mpsc::unbounded_channel();
            self.streams.lock().expect("streams lock").push(tx);
            let ack = self.ack.clone();
            Ok(GrpcStream::new(Box::pin(async_stream::stream! {
                yield Ok(ack);
                while let Some(item) = rx.recv().await {
                    yield item;
                }
            })))
        }
    }

    fn server_check_payload(id: &str) -> Payload {
        server_check_payload_with(id, true)
    }

    fn server_check_payload_with(id: &str, support_ability_negotiation: bool) -> Payload {
        let response = ServerCheckResponse {
            connection_id: Some(id.to_string()),
            support_ability_negotiation,
            ..Default::default()
        };
        GrpcMessageBuilder::new(response)
            .build()
            .into_payload()
            .expect("server check payload")
    }

    fn test_settings() -> ConnectionSettings {
        ConnectionSettings {
            server_check_timeout: Duration::from_millis(50),
            capability_negotiation_timeout: Duration::from_millis(50),
            request_timeout: Duration::from_millis(100),
            health_interval: Duration::from_secs(3600),
            health_retry_times: 1,
            health_timeout: Duration::from_millis(50),
            backlog: 16,
            retry: super::super::backoff::RetryPolicy::for_test(1, 2, 5, 10),
            ..Default::default()
        }
    }

    struct LocalSpawner;

    impl Spawner for LocalSpawner {
        fn spawn(
            &self,
            future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        ) -> tokio::task::JoinHandle<()> {
            tokio::spawn(future)
        }
    }

    /// Spawner that keeps abort handles for every spawned task so a test can
    /// observe whether they were cancelled.
    #[derive(Default)]
    struct AbortRecordingSpawner {
        tasks: Mutex<Vec<tokio::task::AbortHandle>>,
    }

    impl Spawner for AbortRecordingSpawner {
        fn spawn(
            &self,
            future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        ) -> tokio::task::JoinHandle<()> {
            let handle = tokio::spawn(future);
            self.tasks
                .lock()
                .expect("tasks lock")
                .push(handle.abort_handle());
            handle
        }
    }

    async fn wait_until(mut predicate: impl FnMut() -> bool) {
        for _ in 0..10_000 {
            if predicate() {
                return;
            }
            tokio::task::yield_now().await;
            tokio::time::advance(Duration::from_millis(1)).await;
        }
        panic!("condition was not met");
    }

    async fn wait_for_state(handle: &ConnectionHandle, expected: ConnectionState) {
        for _ in 0..10_000 {
            if handle.state() == expected {
                return;
            }
            tokio::task::yield_now().await;
            tokio::time::advance(Duration::from_millis(1)).await;
        }
        panic!(
            "state {expected:?} was not reached, current: {:?}",
            handle.state()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_startup_budget_exhausted_returns_error() {
        let connector = ScriptedConnector::failing();
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector.clone(),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 3 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        let result = waiter.wait().await;
        assert!(result.is_err());
        assert_eq!(connector.connects.load(Ordering::SeqCst), 3);
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_single_attempt_budget_connects_exactly_once() {
        let connector = ScriptedConnector::failing();
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector.clone(),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 1 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        assert!(waiter.wait().await.is_err());
        assert_eq!(connector.connects.load(Ordering::SeqCst), 1);
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_zero_budget_behaves_like_one_attempt() {
        let connector = ScriptedConnector::failing();
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector.clone(),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 0 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        assert!(waiter.wait().await.is_err());
        assert_eq!(connector.connects.load(Ordering::SeqCst), 1);
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_background_mode_returns_immediately_and_keeps_retrying() {
        let connector = ScriptedConnector::failing();
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector.clone(),
            HandlerMap::new(),
            test_settings(),
            StartupMode::Background,
            None,
            None,
            Arc::new(LocalSpawner),
        );
        assert!(waiter.wait().await.is_ok());
        wait_until(|| connector.connects.load(Ordering::SeqCst) >= 4).await;
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_ready_connection_serves_requests_and_exposes_abilities() {
        let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1")));
        let business = GrpcMessageBuilder::new(
            crate::common::remote::grpc::message::response::HealthCheckResponse::default(),
        )
        .build()
        .into_payload()
        .expect("business payload");
        transport.push_unary(Ok(business.clone()));

        let connector = ScriptedConnector::once(transport);
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector,
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 1 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        waiter.wait().await.expect("ready");
        assert_eq!(
            handle
                .server_abilities()
                .get("supportPersistentInstanceByGrpc"),
            Some(&true)
        );
        let response = handle
            .send_request(Payload::default())
            .await
            .expect("business response");
        assert_eq!(response, business);
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_inflight_request_fails_when_stream_closes() {
        let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1")));
        let connector = ScriptedConnector::once(transport.clone());
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector,
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 1 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        waiter.wait().await.expect("ready");

        let request_handle = handle.clone();
        let request =
            tokio::spawn(async move { request_handle.send_request(Payload::default()).await });
        wait_until(|| transport.pending_calls.load(Ordering::SeqCst) > 0).await;
        transport.close_stream();

        let result = tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .expect("request should finish")
            .expect("request task");
        assert!(
            matches!(&result, Err(Error::ClientUnhealthy(message)) if message == "connection lost"),
            "expected the in-flight cancellation path, got {result:?}"
        );
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_abandoned_request_cancels_the_transport_call() {
        let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1")));
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            ScriptedConnector::once(transport.clone()),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 1 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        waiter.wait().await.expect("ready");

        // No scripted unary response, so the transport call pends past the
        // caller's deadline.
        let result = handle.send_request(Payload::default()).await;
        assert!(
            matches!(&result, Err(Error::ClientUnhealthy(message)) if message == "connection request timeout"),
            "unexpected result: {result:?}"
        );
        assert_eq!(transport.pending_calls.load(Ordering::SeqCst), 1);

        // The abandoned unary must be dropped instead of running detached.
        wait_until(|| transport.abandoned_calls.load(Ordering::SeqCst)).await;
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_pending_connect_is_bounded_by_request_timeout() {
        let settings = ConnectionSettings {
            request_timeout: Duration::from_millis(10),
            ..test_settings()
        };
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            PendingConnector,
            HandlerMap::new(),
            settings,
            StartupMode::WaitForReady { max_attempts: 1 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        let result = tokio::time::timeout(Duration::from_secs(1), waiter.wait())
            .await
            .expect("a pending connect must not hang the waiter");
        let error = result.expect_err("pending connect must fail");
        assert!(
            error.to_string().contains("connect timeout"),
            "unexpected error: {error}"
        );
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_shutdown_during_startup_resolves_the_waiter() {
        let settings = ConnectionSettings {
            request_timeout: Duration::from_secs(3600),
            ..test_settings()
        };
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            PendingConnector,
            HandlerMap::new(),
            settings,
            StartupMode::WaitForReady { max_attempts: 3 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        drop(handle);

        let result = tokio::time::timeout(Duration::from_secs(1), waiter.wait())
            .await
            .expect("waiter must resolve promptly after shutdown");
        assert!(result.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn test_dropping_active_connection_tears_down_pump_and_health() {
        let spawner = AbortRecordingSpawner::default();
        let pump = spawner.spawn(Box::pin(std::future::pending::<()>()));
        let health = spawner.spawn(Box::pin(std::future::pending::<()>()));
        let (events_tx, events_rx) = mpsc::channel(1);
        drop(events_tx);
        let (cancel, cancel_rx) = watch::channel(false);

        let active = ActiveConnection {
            transport: Arc::new(ScriptedTransport::new(server_check_payload("c-1"))),
            connection_id: "c-1".to_string(),
            pump,
            health,
            events_rx,
            last_active: Arc::new(AtomicU64::new(now_millis())),
            cancel,
        };
        drop(active);

        assert!(
            *cancel_rx.borrow(),
            "drop must signal in-flight cancellation"
        );
        wait_until(|| {
            spawner
                .tasks
                .lock()
                .expect("tasks lock")
                .iter()
                .all(tokio::task::AbortHandle::is_finished)
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn test_listeners_fire_on_connect_and_disconnect() {
        let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1")));
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let connected = {
            let events = events.clone();
            Arc::new(move |id: String| events.lock().expect("events").push(format!("up:{id}")))
        };
        let disconnected = {
            let events = events.clone();
            Arc::new(move |id: String| events.lock().expect("events").push(format!("down:{id}")))
        };
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            ScriptedConnector::once(transport.clone()),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 1 },
            Some(connected),
            Some(disconnected),
            Arc::new(LocalSpawner),
        );
        waiter.wait().await.expect("ready");
        wait_until(|| events.lock().expect("events").len() == 1).await;
        assert_eq!(events.lock().expect("events").as_slice(), &["up:c-1"]);

        transport.close_stream();
        wait_until(|| events.lock().expect("events").len() == 2).await;
        assert_eq!(
            events.lock().expect("events").as_slice(),
            &["up:c-1", "down:c-1"]
        );
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_reconnect_fires_listeners_for_each_connection() {
        let first = Arc::new(ScriptedTransport::new(server_check_payload("c-1")));
        let second = Arc::new(ScriptedTransport::new(server_check_payload("c-2")));
        let connector = ScriptedConnector::with_transports(vec![first.clone(), second.clone()]);
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let connected = {
            let events = events.clone();
            Arc::new(move |id: String| events.lock().expect("events").push(format!("up:{id}")))
        };
        let disconnected = {
            let events = events.clone();
            Arc::new(move |id: String| events.lock().expect("events").push(format!("down:{id}")))
        };
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector.clone(),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 1 },
            Some(connected),
            Some(disconnected),
            Arc::new(LocalSpawner),
        );
        waiter.wait().await.expect("ready");
        wait_until(|| events.lock().expect("events").len() == 1).await;
        assert_eq!(events.lock().expect("events").as_slice(), &["up:c-1"]);

        first.close_stream();
        wait_until(|| events.lock().expect("events").len() == 3).await;
        assert_eq!(
            events.lock().expect("events").as_slice(),
            &["up:c-1", "down:c-1", "up:c-2"]
        );
        assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_backoff_state_resets_after_a_stable_connection() {
        let first = Arc::new(ScriptedTransport::new(server_check_payload("c-1")));
        let second = Arc::new(ScriptedTransport::new(server_check_payload("c-2")));
        let connector = ScriptedConnector::from_results(vec![
            Err(Error::ErrResult("connect failed".to_string())),
            Ok(first.clone()),
            Ok(second),
        ]);
        let (handle, _waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector.clone(),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 3 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        wait_for_state(&handle, ConnectionState::Backoff { attempt: 1 }).await;
        wait_for_state(
            &handle,
            ConnectionState::Ready {
                connection_id: "c-1".to_string(),
            },
        )
        .await;

        // test settings reset the counter after 10ms of stability.
        tokio::time::advance(Duration::from_millis(11)).await;
        first.close_stream();

        wait_for_state(&handle, ConnectionState::Backoff { attempt: 1 }).await;
        assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
        drop(handle);
    }

    #[tokio::test(start_paused = true)]
    async fn test_unsupported_server_fails_startup_without_retrying() {
        let transport = Arc::new(ScriptedTransport::new(server_check_payload_with(
            "c-1", false,
        )));
        let connector = ScriptedConnector::with_transports(vec![transport]);
        let (handle, waiter) = ConnectionHandle::start_with(
            "test".to_string(),
            connector.clone(),
            HandlerMap::new(),
            test_settings(),
            StartupMode::WaitForReady { max_attempts: 3 },
            None,
            None,
            Arc::new(LocalSpawner),
        );
        let error = waiter
            .wait()
            .await
            .expect_err("an unsupported server must fail startup");
        assert!(
            matches!(error, Error::UnsupportedServerVersion(_)),
            "unexpected error: {error}"
        );
        assert_eq!(connector.connects.load(Ordering::SeqCst), 1);
        assert_eq!(handle.state(), ConnectionState::Stopped);
        drop(handle);
    }
}
