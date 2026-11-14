use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::error;

use crate::api::error::Error;
use crate::common::util::now_millis;
use crate::nacos_proto::v2::Payload;

use super::config::ConnectionSettings;
use super::handshake;
use super::health;
use super::{ConnectionEvent, Connector, HandlerMap, Spawner, Transport, error_messages};

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
                            _ = cancel.changed() => Err(Error::ClientUnhealthy(error_messages::CONNECTION_LOST.to_string())),
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
                        error_messages::NOT_READY.to_string(),
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
                    self.publish(ConnectionState::Ready { connection_id });
                    if let Some(tx) = self.startup_tx.take() {
                        let _ = tx.send(Ok(()));
                    }

                    let result = self.serve(active, &mut shutdown_rx).await;
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
                    // retrying, so a waiting startup fails immediately with that
                    // error. Background (emergency) startup keeps retrying, and
                    // once the connection has been ready at least once the
                    // server may have been replaced, so both are logged and
                    // retried instead of stopping permanently.
                    if !ever_ready
                        && matches!(self.startup_mode, StartupMode::WaitForReady { .. })
                        && matches!(&error, Error::UnsupportedServerVersion(_))
                    {
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
                                    error_messages::NOT_READY.to_string(),
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
mod tests;
