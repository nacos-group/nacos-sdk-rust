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

#[allow(dead_code)] // consumed by the connection supervisor in B4
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
#[allow(dead_code)] // only constructed by `negotiate`, which the supervisor calls in B4
struct PumpGuard(Option<JoinHandle<()>>);

impl Drop for PumpGuard {
    fn drop(&mut self) {
        if let Some(pump) = self.0.as_ref() {
            pump.abort();
        }
    }
}

#[allow(dead_code)] // called by the connection supervisor in B4
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

    let (local_tx, mut local_rx) = mpsc::channel::<Payload>(settings.backlog);
    local_tx
        .send(setup_payload)
        .await
        .map_err(|_| Error::ErrResult("bi stream outbound channel is closed".to_string()))?;

    let request_stream = GrpcStream::new(Box::pin(async_stream::stream! {
        while let Some(payload) = local_rx.recv().await {
            yield payload;
        }
    }));
    let server_stream = transport.open_stream(request_stream).await?;

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

    match tokio::time::timeout(settings.capability_negotiation_timeout, setup_ack_rx).await {
        Ok(Ok(abilities)) => {
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
    }

    impl ScriptedTransport {
        fn new(unary: Vec<Result<Payload, Error>>, ack: Option<Payload>, pending: bool) -> Self {
            Self {
                unary: Mutex::new(unary.into()),
                ack,
                pending: AtomicBool::new(pending),
                open_calls: AtomicUsize::new(0),
                last_setup: Mutex::new(None),
            }
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
        negotiate(
            transport,
            settings,
            Arc::new(HashMap::new()),
            events_tx,
            last_active,
            spawner.as_ref(),
        )
        .await
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
}
