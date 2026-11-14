pub(crate) mod backoff;
pub(crate) mod config;
pub(crate) mod handshake;
pub(crate) mod health;
pub(crate) mod stream;
pub(crate) mod supervisor;

#[cfg(test)]
pub(crate) mod test_support;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use async_trait::async_trait;
use futures::Future;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;

use crate::api::error::Error;
use crate::common::remote::grpc::nacos_grpc_service::{GrpcStream, ServerRequestHandler};
use crate::nacos_proto::v2::Payload;

pub(crate) use config::ConnectionSettings;
use supervisor::{ConnectionState, OutboundRequest, StartupMode, StartupWaiter, Supervisor};

pub(crate) type HandlerMap = HashMap<String, Arc<dyn ServerRequestHandler>>;

pub(crate) mod error_messages {
    pub(crate) const NOT_READY: &str = "connection is not ready";
    pub(crate) const SUPERVISOR_STOPPED: &str = "connection supervisor stopped";
    pub(crate) const CONNECTION_LOST: &str = "connection lost";
    pub(crate) const REQUEST_TIMEOUT: &str = "connection request timeout";
    pub(crate) const REQUEST_CANCELLED: &str = "connection request cancelled";
}

#[derive(Debug)]
pub(crate) enum ConnectionEvent {
    StreamClosed(Error),
    Unhealthy(Error),
}

#[async_trait]
pub(crate) trait Transport: Send + Sync + 'static {
    async fn unary(&self, payload: Payload) -> Result<Payload, Error>;

    async fn open_stream(
        &self,
        request: GrpcStream<Payload>,
    ) -> Result<GrpcStream<Result<Payload, Error>>, Error>;

    /// Server address used in diagnostics. Test transports keep the default.
    fn server_addr(&self) -> String {
        "unknown".to_string()
    }
}

#[async_trait]
pub(crate) trait Connector: Send + 'static {
    async fn connect(&mut self) -> Result<Arc<dyn Transport>, Error>;
}

pub(crate) trait Spawner: Send + Sync + 'static {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()>;
}

pub(crate) struct ExecutorSpawner;

impl Spawner for ExecutorSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        crate::common::executor::spawn(future)
    }
}

pub(crate) type ConnectionListener = Arc<dyn Fn(String) + Send + Sync + 'static>;

struct HandleInner {
    tx: tokio::sync::mpsc::Sender<OutboundRequest>,
    state: watch::Receiver<ConnectionState>,
    shutdown: watch::Sender<bool>,
    request_timeout: Duration,
    server_abilities: Arc<RwLock<HashMap<String, bool>>>,
}

impl Drop for HandleInner {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

#[derive(Clone)]
pub(crate) struct ConnectionHandle {
    inner: Arc<HandleInner>,
}

impl ConnectionHandle {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_with<C: Connector + 'static>(
        id: String,
        connector: C,
        handler_map: HandlerMap,
        settings: ConnectionSettings,
        startup_mode: StartupMode,
        connected_listener: Option<ConnectionListener>,
        disconnected_listener: Option<ConnectionListener>,
        spawner: Arc<dyn Spawner>,
    ) -> (Self, StartupWaiter) {
        let (tx, rx) = tokio::sync::mpsc::channel(settings.backlog.max(1));
        let (state_tx, state_rx) = watch::channel(ConnectionState::Connecting);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (startup_tx, startup_rx) = match startup_mode {
            StartupMode::WaitForReady { .. } => {
                let (tx, rx) = oneshot::channel();
                (Some(tx), Some(rx))
            }
            StartupMode::Background => (None, None),
        };

        if let Some(listener) = connected_listener {
            spawn_listener(&spawner, state_tx.subscribe(), true, listener);
        }
        if let Some(listener) = disconnected_listener {
            spawn_listener(&spawner, state_tx.subscribe(), false, listener);
        }

        let server_abilities = Arc::new(RwLock::new(HashMap::new()));
        let supervisor = Supervisor {
            id: id.clone(),
            connector,
            handler_map: Arc::new(handler_map),
            settings: settings.clone(),
            rx,
            state_tx,
            startup_tx,
            startup_mode,
            spawner: spawner.clone(),
            server_abilities: server_abilities.clone(),
        };
        spawner.spawn(Box::pin(supervisor.run(shutdown_rx)));

        let inner = Arc::new(HandleInner {
            tx,
            state: state_rx,
            shutdown: shutdown_tx,
            request_timeout: settings.request_timeout,
            server_abilities,
        });
        (Self { inner }, StartupWaiter(startup_rx))
    }

    pub(crate) fn server_abilities(&self) -> HashMap<String, bool> {
        self.inner
            .server_abilities
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn state(&self) -> ConnectionState {
        self.inner.state.borrow().clone()
    }
}

fn spawn_listener(
    spawner: &Arc<dyn Spawner>,
    mut receiver: watch::Receiver<ConnectionState>,
    connected: bool,
    listener: ConnectionListener,
) {
    let mut previous = receiver.borrow().clone();
    spawner.spawn(Box::pin(async move {
        while receiver.changed().await.is_ok() {
            let current = receiver.borrow().clone();
            if connected {
                if let ConnectionState::Ready { connection_id } = &current
                    && !matches!(previous, ConnectionState::Ready { .. })
                {
                    listener(connection_id.clone());
                }
            } else if let ConnectionState::Ready { connection_id } = &previous
                && !matches!(current, ConnectionState::Ready { .. })
            {
                listener(connection_id.clone());
            }
            previous = current;
        }
    }));
}

#[allow(clippy::disallowed_methods)] // mockall's generated MockSendRequest calls unwrap
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub(crate) trait SendRequest: Send {
    async fn send_request(&self, request: Payload) -> Result<Payload, Error>;
}

#[async_trait]
impl SendRequest for ConnectionHandle {
    async fn send_request(&self, request: Payload) -> Result<Payload, Error> {
        let deadline = tokio::time::Instant::now() + self.inner.request_timeout;
        let mut state = self.inner.state.clone();
        let ready = async {
            loop {
                match &*state.borrow() {
                    ConnectionState::Ready { .. } => return Ok(()),
                    ConnectionState::Stopped => {
                        return Err(Error::ClientUnhealthy(
                            error_messages::SUPERVISOR_STOPPED.to_string(),
                        ));
                    }
                    _ => {}
                }
                state.changed().await.map_err(|_| {
                    Error::ClientUnhealthy(error_messages::SUPERVISOR_STOPPED.to_string())
                })?;
            }
        };
        tokio::time::timeout_at(deadline, ready)
            .await
            .map_err(|_| Error::ClientUnhealthy(error_messages::NOT_READY.to_string()))??;

        let (reply, response) = oneshot::channel();
        tokio::time::timeout_at(
            deadline,
            self.inner.tx.send(OutboundRequest {
                payload: request,
                reply,
            }),
        )
        .await
        .map_err(|_| Error::ClientUnhealthy("connection request queue is full".to_string()))?
        .map_err(|_| Error::ClientUnhealthy(error_messages::SUPERVISOR_STOPPED.to_string()))?;

        tokio::time::timeout_at(deadline, response)
            .await
            .map_err(|_| Error::ClientUnhealthy(error_messages::REQUEST_TIMEOUT.to_string()))?
            .map_err(|_| Error::ClientUnhealthy(error_messages::REQUEST_CANCELLED.to_string()))?
    }
}
