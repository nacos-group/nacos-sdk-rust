use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info};

use crate::api::error::Error;
use crate::common::remote::grpc::message::request::{ConnectionSetupRequest, ServerCheckRequest};
use crate::common::remote::grpc::message::response::ServerCheckResponse;
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};
use crate::common::remote::grpc::nacos_grpc_service::GrpcStream;
use crate::nacos_proto::v2::Payload;

use super::stream;
use super::{ConnectionEvent, ConnectionSettings, HandlerMap, Spawner, Transport};

pub(crate) struct Negotiated {
    pub(crate) transport: Arc<dyn Transport>,
    pub(crate) connection_id: String,
    pub(crate) abilities: HashMap<String, bool>,
    pub(crate) pump: JoinHandle<()>,
}

// `Transport` is not `Debug`, so the handle is printed as a placeholder instead
// of derived.
impl std::fmt::Debug for Negotiated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Negotiated")
            .field("transport", &"<dyn Transport>")
            .field("connection_id", &self.connection_id)
            .field("abilities", &self.abilities)
            .field("pump", &self.pump)
            .finish()
    }
}

/// Aborts the stream pump if the handshake future is dropped (e.g. shutdown
/// during capability negotiation) before ownership is transferred out.
struct PumpGuard(Option<JoinHandle<()>>);

impl Drop for PumpGuard {
    fn drop(&mut self) {
        if let Some(pump) = self.0.as_ref() {
            pump.abort();
        }
    }
}

pub(crate) async fn negotiate(
    transport: Arc<dyn Transport>,
    settings: &ConnectionSettings,
    handler_map: Arc<HandlerMap>,
    events_tx: mpsc::Sender<ConnectionEvent>,
    last_active: Arc<AtomicU64>,
    spawner: &dyn Spawner,
) -> Result<Negotiated, Error> {
    let server_check = ServerCheckRequest::new();
    let request = GrpcMessageBuilder::new(server_check)
        .build()
        .into_payload()?;
    let response = tokio::time::timeout(settings.server_check_timeout, transport.unary(request))
        .await
        .map_err(|_| Error::ErrResult("server check request timeout".to_string()))??;
    let response = GrpcMessage::<ServerCheckResponse>::from_payload(response)?.into_body();
    let connection_id = response.connection_id.ok_or_else(|| {
        Error::ErrResult("server check response has no connection id".to_string())
    })?;
    if !response.support_ability_negotiation {
        return Err(Error::UnsupportedServerVersion(
            "ability negotiation is not supported; nacos-server >= 2.3.0 is required".to_string(),
        ));
    }
    debug!("server check passed, connection id: {connection_id}");

    let setup = ConnectionSetupRequest {
        client_version: settings.client_version.clone(),
        tenant: settings.namespace.clone(),
        labels: settings.labels.clone(),
        ability_table: Some(HashMap::new()),
        ..Default::default()
    };
    let setup_payload = GrpcMessageBuilder::new(setup).build().into_payload()?;

    let (local_tx, mut local_rx) = mpsc::channel::<Payload>(settings.backlog.max(1));
    local_tx
        .send(setup_payload)
        .await
        .map_err(|_| Error::ErrResult("bi stream outbound channel is closed".to_string()))?;

    let request_stream = GrpcStream::new(Box::pin(async_stream::stream! {
        while let Some(payload) = local_rx.recv().await {
            yield payload;
        }
    }));
    let server_stream = tokio::time::timeout(
        settings.capability_negotiation_timeout,
        transport.open_stream(request_stream),
    )
    .await
    .map_err(|_| Error::ErrResult("open bi stream timeout".to_string()))??;

    let (setup_ack_tx, setup_ack_rx) = oneshot::channel();
    let pump = spawner.spawn(Box::pin(stream::run(
        server_stream,
        local_tx,
        handler_map,
        setup_ack_tx,
        events_tx,
        last_active,
    )));
    let mut pump_guard = PumpGuard(Some(pump));

    // Layers: timeout -> oneshot receive -> setup-ack outcome.
    match tokio::time::timeout(settings.capability_negotiation_timeout, setup_ack_rx).await {
        Ok(Ok(Ok(abilities))) => {
            let abilities = abilities.unwrap_or_default();
            info!("capability negotiation completed, server abilities: {abilities:?}");
            let pump = pump_guard.0.take().expect("pump handle must be present");
            Ok(Negotiated {
                transport,
                connection_id,
                abilities,
                pump,
            })
        }
        Ok(Ok(Err(message))) => Err(Error::ErrResult(message)),
        Ok(Err(_)) => Err(Error::ErrResult(
            "setup ack channel closed before response".to_string(),
        )),
        Err(_) => Err(Error::ErrResult(
            "capability negotiation timeout".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use futures::StreamExt;

    use super::*;
    use crate::common::remote::grpc::message::request::SetupAckRequest;
    use crate::common::remote::grpc::message::response::ServerCheckResponse;

    struct ScriptedTransport {
        unary: Mutex<VecDeque<Result<Payload, Error>>>,
        ack: Option<Payload>,
        pending: AtomicBool,
        open_calls: AtomicUsize,
        last_setup: Mutex<Option<Payload>>,
        stream_close_reason: Option<String>,
    }

    impl ScriptedTransport {
        fn new(unary: Vec<Result<Payload, Error>>, ack: Option<Payload>, pending: bool) -> Self {
            Self {
                unary: Mutex::new(unary.into()),
                ack,
                pending: AtomicBool::new(pending),
                open_calls: AtomicUsize::new(0),
                last_setup: Mutex::new(None),
                stream_close_reason: None,
            }
        }

        /// Makes the server stream fail immediately, before any setup ack.
        fn with_stream_close_reason(mut self, reason: impl Into<String>) -> Self {
            self.stream_close_reason = Some(reason.into());
            self
        }

        fn open_calls(&self) -> usize {
            self.open_calls.load(Ordering::SeqCst)
        }

        fn last_setup_json(&self) -> serde_json::Value {
            let payload = self
                .last_setup
                .lock()
                .expect("setup lock")
                .clone()
                .expect("setup payload");
            serde_json::from_slice(&payload.body.expect("body").value).expect("setup json")
        }
    }

    #[async_trait]
    impl Transport for ScriptedTransport {
        async fn unary(&self, _payload: Payload) -> Result<Payload, Error> {
            self.unary
                .lock()
                .expect("unary lock")
                .pop_front()
                .expect("scripted unary response")
        }

        async fn open_stream(
            &self,
            mut request: GrpcStream<Payload>,
        ) -> Result<GrpcStream<Result<Payload, Error>>, Error> {
            self.open_calls.fetch_add(1, Ordering::SeqCst);
            let setup = request.next().await.expect("setup payload");
            *self.last_setup.lock().expect("setup lock") = Some(setup);
            if let Some(reason) = self.stream_close_reason.clone() {
                return Ok(GrpcStream::new(Box::pin(futures::stream::iter(vec![Err(
                    Error::ErrResult(reason),
                )]))));
            }
            if self.pending.load(Ordering::SeqCst) {
                return Ok(GrpcStream::new(Box::pin(futures::stream::pending::<
                    Result<Payload, Error>,
                >())));
            }
            let ack = self.ack.clone();
            Ok(GrpcStream::new(Box::pin(futures::stream::iter(
                ack.into_iter().map(Ok).collect::<Vec<_>>(),
            ))))
        }
    }

    /// Spawner that records the spawned pump so a test can assert its terminal
    /// state. `JoinHandle` is not cloneable and `negotiate` keeps the returned
    /// handle in its `PumpGuard`, so the returned handle is a linked wrapper
    /// that aborts the recorded pump task when it is dropped or aborted itself.
    #[derive(Default)]
    struct RecordingSpawner {
        handles: Mutex<Vec<JoinHandle<()>>>,
    }

    impl RecordingSpawner {
        fn new() -> Self {
            Self::default()
        }

        fn pop_handle(&self) -> JoinHandle<()> {
            self.handles
                .lock()
                .expect("handles lock")
                .pop()
                .expect("spawned pump handle")
        }
    }

    impl Spawner for RecordingSpawner {
        fn spawn(
            &self,
            future: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
        ) -> JoinHandle<()> {
            let pump = tokio::spawn(future);
            let abort = pump.abort_handle();
            self.handles.lock().expect("handles lock").push(pump);
            tokio::spawn(async move {
                struct AbortOnDrop(tokio::task::AbortHandle);
                impl Drop for AbortOnDrop {
                    fn drop(&mut self) {
                        self.0.abort();
                    }
                }
                let _guard = AbortOnDrop(abort);
                std::future::pending::<()>().await;
            })
        }
    }

    /// Transport whose `open_stream` never returns, to pin the negotiation-side
    /// deadline around opening the bi-stream.
    struct HangingOpenTransport {
        server_check: Payload,
    }

    #[async_trait]
    impl Transport for HangingOpenTransport {
        async fn unary(&self, _payload: Payload) -> Result<Payload, Error> {
            Ok(self.server_check.clone())
        }

        async fn open_stream(
            &self,
            _request: GrpcStream<Payload>,
        ) -> Result<GrpcStream<Result<Payload, Error>>, Error> {
            std::future::pending::<Result<GrpcStream<Result<Payload, Error>>, Error>>().await
        }
    }

    fn server_check_payload(id: &str, support: bool) -> Payload {
        let response = ServerCheckResponse {
            connection_id: Some(id.to_string()),
            support_ability_negotiation: support,
            ..Default::default()
        };
        GrpcMessageBuilder::new(response)
            .build()
            .into_payload()
            .expect("server check payload")
    }

    fn setup_ack_payload(table: HashMap<String, bool>) -> Payload {
        let ack = SetupAckRequest {
            ability_table: Some(table),
            ..Default::default()
        };
        GrpcMessageBuilder::new(ack)
            .build()
            .into_payload()
            .expect("setup ack payload")
    }

    fn test_settings(timeout_ms: u64) -> ConnectionSettings {
        ConnectionSettings {
            server_check_timeout: Duration::from_millis(timeout_ms),
            capability_negotiation_timeout: Duration::from_millis(timeout_ms),
            ..Default::default()
        }
    }

    fn test_spawner() -> Arc<dyn Spawner> {
        struct LocalSpawner;
        impl Spawner for LocalSpawner {
            fn spawn(
                &self,
                future: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            ) -> tokio::task::JoinHandle<()> {
                tokio::spawn(future)
            }
        }
        Arc::new(LocalSpawner)
    }

    async fn run(
        transport: Arc<ScriptedTransport>,
        settings: &ConnectionSettings,
    ) -> Result<Negotiated, Error> {
        let spawner = test_spawner();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(8);
        let last_active = Arc::new(std::sync::atomic::AtomicU64::new(0));
        // Fast-fail instead of hanging forever if the setup ordering regresses.
        tokio::time::timeout(
            Duration::from_secs(1),
            negotiate(
                transport,
                settings,
                Arc::new(HashMap::new()),
                events_tx,
                last_active,
                spawner.as_ref(),
            ),
        )
        .await
        .expect("negotiation must not hang")
    }

    #[tokio::test]
    async fn test_rejects_server_without_ability_negotiation() {
        let transport = Arc::new(ScriptedTransport::new(
            vec![Ok(server_check_payload("c-1", false))],
            None,
            false,
        ));
        let result = run(transport.clone(), &test_settings(50)).await;
        assert!(matches!(result, Err(Error::UnsupportedServerVersion(_))));
        assert_eq!(transport.open_calls(), 0);
    }

    #[tokio::test]
    async fn test_negotiates_and_records_server_abilities() {
        let transport = Arc::new(ScriptedTransport::new(
            vec![Ok(server_check_payload("c-1", true))],
            Some(setup_ack_payload(HashMap::from([(
                "supportPersistentInstanceByGrpc".to_string(),
                true,
            )]))),
            false,
        ));
        let negotiated = run(transport.clone(), &test_settings(50))
            .await
            .expect("negotiation should succeed");
        assert_eq!(negotiated.connection_id, "c-1");
        assert_eq!(
            negotiated.abilities.get("supportPersistentInstanceByGrpc"),
            Some(&true)
        );
        let setup = transport.last_setup_json();
        assert_eq!(setup["abilityTable"], serde_json::json!({}));
        assert!(setup.get("abilities").is_none());
    }

    #[tokio::test]
    async fn test_setup_ack_timeout_is_reported() {
        let transport = Arc::new(ScriptedTransport::new(
            vec![Ok(server_check_payload("c-1", true))],
            None,
            true,
        ));
        let result = run(transport, &test_settings(10)).await;
        let error = result.expect_err("timeout expected");
        assert!(
            error.to_string().contains("capability negotiation timeout"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn test_stream_closed_before_setup_ack_surfaces_the_cause() {
        let transport = Arc::new(
            ScriptedTransport::new(vec![Ok(server_check_payload("c-1", true))], None, false)
                .with_stream_close_reason("connection reset by peer"),
        );
        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(8);
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            negotiate(
                transport,
                &test_settings(100),
                Arc::new(HashMap::new()),
                events_tx,
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
                test_spawner().as_ref(),
            ),
        )
        .await
        .expect("negotiation must not hang");
        let error = result.expect_err("stream closure before ack must fail the handshake");
        let message = error.to_string();
        assert!(
            message.contains("closed before setup ack"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("connection reset by peer"),
            "closure cause is missing: {message}"
        );
        assert!(
            !message.contains("setup ack channel closed before response"),
            "generic channel-closed message was reported instead: {message}"
        );

        let event = events_rx.recv().await.expect("terminal event");
        assert!(matches!(event, ConnectionEvent::StreamClosed(_)));
    }

    #[tokio::test]
    async fn test_open_stream_timeout_is_reported() {
        let transport = Arc::new(HangingOpenTransport {
            server_check: server_check_payload("c-1", true),
        });
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(8);
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            negotiate(
                transport,
                &test_settings(10),
                Arc::new(HashMap::new()),
                events_tx,
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
                test_spawner().as_ref(),
            ),
        )
        .await
        .expect("negotiation must not hang");
        let error = result.expect_err("open stream timeout expected");
        assert!(
            error.to_string().contains("open bi stream timeout"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn test_pump_is_aborted_when_negotiation_fails() {
        let transport = Arc::new(ScriptedTransport::new(
            vec![Ok(server_check_payload("c-1", true))],
            None,
            true,
        ));
        let spawner = Arc::new(RecordingSpawner::new());
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(8);
        let result = negotiate(
            transport,
            &test_settings(10),
            Arc::new(HashMap::new()),
            events_tx,
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            spawner.as_ref(),
        )
        .await;
        let error = result.expect_err("timeout expected");
        assert!(
            error.to_string().contains("capability negotiation timeout"),
            "unexpected error: {error}"
        );

        let handle = spawner.pop_handle();
        let join = tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("pump handle must settle once the handshake fails")
            .expect_err("pump task must be cancelled");
        assert!(join.is_cancelled(), "unexpected join error: {join}");
    }
}
