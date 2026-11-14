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
use crate::common::remote::grpc::connection::test_support::{server_check_payload, test_spawner};
use crate::common::remote::grpc::connection::{HandlerMap, Spawner, Transport};
use crate::common::remote::grpc::message::GrpcMessageBuilder;
use crate::common::remote::grpc::message::request::SetupAckRequest;
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

/// Connector that always succeeds with a pre-2.3 server which reports no
/// ability negotiation.
#[derive(Clone, Default)]
struct AlwaysOldServerConnector {
    connects: Arc<AtomicUsize>,
}

#[async_trait]
impl Connector for AlwaysOldServerConnector {
    async fn connect(&mut self) -> Result<Arc<dyn Transport>, Error> {
        self.connects.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(ScriptedTransport::new(old_server_check_payload())))
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

/// Server-check response of a pre-2.3 server: no ability negotiation.
fn old_server_check_payload() -> Payload {
    server_check_payload("c-old", false)
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
        test_spawner(),
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
        test_spawner(),
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
        test_spawner(),
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
        test_spawner(),
    );
    assert!(waiter.wait().await.is_ok());
    wait_until(|| connector.connects.load(Ordering::SeqCst) >= 4).await;
    drop(handle);
}

#[tokio::test(start_paused = true)]
async fn test_ready_connection_serves_requests_and_exposes_abilities() {
    let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1", true)));
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
        test_spawner(),
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
    let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1", true)));
    let connector = ScriptedConnector::once(transport.clone());
    let (handle, waiter) = ConnectionHandle::start_with(
        "test".to_string(),
        connector,
        HandlerMap::new(),
        test_settings(),
        StartupMode::WaitForReady { max_attempts: 1 },
        None,
        None,
        test_spawner(),
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
        matches!(&result, Err(Error::ClientUnhealthy(message)) if message == error_messages::CONNECTION_LOST),
        "expected the in-flight cancellation path, got {result:?}"
    );
    drop(handle);
}

#[tokio::test(start_paused = true)]
async fn test_abandoned_request_cancels_the_transport_call() {
    let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1", true)));
    let (handle, waiter) = ConnectionHandle::start_with(
        "test".to_string(),
        ScriptedConnector::once(transport.clone()),
        HandlerMap::new(),
        test_settings(),
        StartupMode::WaitForReady { max_attempts: 1 },
        None,
        None,
        test_spawner(),
    );
    waiter.wait().await.expect("ready");

    // No scripted unary response, so the transport call pends past the
    // caller's deadline.
    let result = handle.send_request(Payload::default()).await;
    assert!(
        matches!(&result, Err(Error::ClientUnhealthy(message)) if message == error_messages::REQUEST_TIMEOUT),
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
        test_spawner(),
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
        test_spawner(),
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
        transport: Arc::new(ScriptedTransport::new(server_check_payload("c-1", true))),
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
    let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1", true)));
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
        test_spawner(),
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
    let first = Arc::new(ScriptedTransport::new(server_check_payload("c-1", true)));
    let second = Arc::new(ScriptedTransport::new(server_check_payload("c-2", true)));
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
        test_spawner(),
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
    let first = Arc::new(ScriptedTransport::new(server_check_payload("c-1", true)));
    let second = Arc::new(ScriptedTransport::new(server_check_payload("c-2", true)));
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
        test_spawner(),
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
    let transport = Arc::new(ScriptedTransport::new(server_check_payload("c-1", false)));
    let connector = ScriptedConnector::with_transports(vec![transport]);
    let (handle, waiter) = ConnectionHandle::start_with(
        "test".to_string(),
        connector.clone(),
        HandlerMap::new(),
        test_settings(),
        StartupMode::WaitForReady { max_attempts: 3 },
        None,
        None,
        test_spawner(),
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

#[tokio::test(start_paused = true)]
async fn test_background_mode_keeps_retrying_unsupported_servers() {
    let connector = AlwaysOldServerConnector::default();
    let (handle, waiter) = ConnectionHandle::start_with(
        "test".to_string(),
        connector.clone(),
        HandlerMap::new(),
        test_settings(),
        StartupMode::Background,
        None,
        None,
        test_spawner(),
    );
    assert!(waiter.wait().await.is_ok());
    wait_until(|| connector.connects.load(Ordering::SeqCst) >= 2).await;
    assert_ne!(
        handle.state(),
        ConnectionState::Stopped,
        "emergency startup must keep retrying instead of stopping"
    );
    assert!(connector.connects.load(Ordering::SeqCst) >= 2);
    drop(handle);
}
