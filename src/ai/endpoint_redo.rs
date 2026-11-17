use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::warn;

use crate::ai::message::is_permanent_rejection;
use crate::api::error::{Error, Result};
use crate::common::remote::grpc::connection::error_messages;

#[derive(Debug, Clone)]
enum EndpointState {
    PendingRegister,
    Registered { connection_id: Option<String> },
    PendingDeregister,
}

#[derive(Debug)]
struct EndpointEntry {
    version: Option<String>,
    state: EndpointState,
}

#[async_trait]
pub(crate) trait EndpointOps: Send + Sync {
    async fn register(
        &self,
        namespace: &str,
        mcp_name: &str,
        address: &str,
        port: i32,
        version: Option<&str>,
    ) -> Result<()>;

    async fn deregister(
        &self,
        namespace: &str,
        mcp_name: &str,
        address: &str,
        port: i32,
    ) -> Result<()>;
}

struct RedoState {
    entries: HashMap<(String, String, i32), EndpointEntry>,
    last_connection_id: Option<String>,
}

pub(crate) struct EndpointRedo {
    namespace: String,
    ops: Arc<dyn EndpointOps>,
    ops_lock: Mutex<()>,
    // invariant: only touched while ops_lock is held by the current task
    state: std::sync::Mutex<RedoState>,
}

impl EndpointRedo {
    pub(crate) fn new(
        namespace: String,
        ops: Arc<dyn EndpointOps>,
        flush_interval: Duration,
    ) -> Arc<Self> {
        let redo = Arc::new(Self {
            namespace,
            ops,
            ops_lock: Mutex::new(()),
            state: std::sync::Mutex::new(RedoState {
                entries: HashMap::new(),
                last_connection_id: None,
            }),
        });
        let weak = Arc::downgrade(&redo);
        crate::common::executor::spawn(async move {
            loop {
                tokio::time::sleep(flush_interval).await;
                match weak.upgrade() {
                    Some(redo) => redo.flush().await,
                    None => break,
                }
            }
        });
        redo
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, RedoState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) async fn on_connected(&self, connection_id: String) {
        let _guard = self.ops_lock.lock().await;
        let mut state = self.lock_state();
        let is_new = state.last_connection_id.as_deref() != Some(connection_id.as_str());
        state.last_connection_id = Some(connection_id);
        if is_new {
            state
                .entries
                .retain(|_, entry| !matches!(entry.state, EndpointState::PendingDeregister));
        }
    }

    pub(crate) async fn register(
        &self,
        mcp_name: String,
        address: String,
        port: i32,
        version: Option<String>,
    ) -> Result<()> {
        let _guard = self.ops_lock.lock().await;
        let key = (mcp_name, address, port);
        self.lock_state().entries.insert(
            key.clone(),
            EndpointEntry {
                version,
                state: EndpointState::PendingRegister,
            },
        );
        self.try_entry_locked(&key).await
    }

    pub(crate) async fn deregister(
        &self,
        mcp_name: String,
        address: String,
        port: i32,
    ) -> Result<()> {
        let _guard = self.ops_lock.lock().await;
        let key = (mcp_name, address, port);
        {
            let mut state = self.lock_state();
            match state.entries.get_mut(&key) {
                Some(entry) => entry.state = EndpointState::PendingDeregister,
                None => {
                    state.entries.insert(
                        key.clone(),
                        EndpointEntry {
                            version: None,
                            state: EndpointState::PendingDeregister,
                        },
                    );
                }
            }
        }
        self.try_entry_locked(&key).await
    }

    pub(crate) async fn flush(&self) {
        let keys: Vec<(String, String, i32)> = {
            let _guard = self.ops_lock.lock().await;
            self.lock_state().entries.keys().cloned().collect()
        };
        for key in keys {
            let result = {
                let _guard = self.ops_lock.lock().await;
                self.try_entry_locked(&key).await
            };
            if let Err(error) = result {
                let (mcp_name, address, port) = &key;
                warn!(
                    mcp_name = mcp_name,
                    address = address,
                    port = port,
                    "mcp endpoint redo attempt failed: {error}"
                );
            }
        }
    }

    async fn try_entry_locked(&self, key: &(String, String, i32)) -> Result<()> {
        let (mcp_name, address, port) = key;
        enum Action {
            Register {
                version: Option<String>,
                connection_id: Option<String>,
            },
            Deregister,
        }
        let action = {
            let state = self.lock_state();
            let Some(entry) = state.entries.get(key) else {
                return Ok(());
            };
            match &entry.state {
                EndpointState::Registered { connection_id }
                    if *connection_id == state.last_connection_id =>
                {
                    return Ok(());
                }
                EndpointState::PendingDeregister => Action::Deregister,
                _ => Action::Register {
                    version: entry.version.clone(),
                    connection_id: state.last_connection_id.clone(),
                },
            }
        };
        match action {
            Action::Register {
                version,
                connection_id,
            } => {
                self.register_entry_locked(key, version.as_deref(), connection_id)
                    .await
            }
            Action::Deregister => {
                match self
                    .ops
                    .deregister(&self.namespace, mcp_name, address, *port)
                    .await
                {
                    Ok(()) => {}
                    Err(error) if is_disconnect_error(&error) => {
                        self.lock_state().entries.remove(key);
                        return Ok(());
                    }
                    Err(error) if is_permanent_rejection(&error) => {
                        warn!(
                            mcp_name = mcp_name,
                            address = address,
                            port = port,
                            "mcp endpoint deregister rejected permanently: {error}"
                        );
                        self.lock_state().entries.remove(key);
                        return Ok(());
                    }
                    Err(error) => return Err(error),
                }
                self.lock_state().entries.remove(key);
                Ok(())
            }
        }
    }

    async fn register_entry_locked(
        &self,
        key: &(String, String, i32),
        version: Option<&str>,
        connection_id: Option<String>,
    ) -> Result<()> {
        let (mcp_name, address, port) = key;
        self.ops
            .register(&self.namespace, mcp_name, address, *port, version)
            .await?;
        let mut state = self.lock_state();
        state
            .entries
            .get_mut(key)
            .expect("entry is owned by ops_lock")
            .state = EndpointState::Registered { connection_id };
        Ok(())
    }
}

fn is_disconnect_error(error: &Error) -> bool {
    match error {
        Error::NoAvailableServer => true,
        // Client-side queueing/timeout failures do not prove the server dropped
        // the endpoint, so keep them retriable.
        Error::ClientUnhealthy(message) => !matches!(
            message.as_str(),
            error_messages::REQUEST_QUEUE_FULL | error_messages::REQUEST_TIMEOUT
        ),
        Error::TonicGrpcStatus(status) => {
            matches!(
                status.code(),
                tonic::Code::Unavailable | tonic::Code::Cancelled
            )
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use tokio::sync::Notify;

    #[derive(Default)]
    struct FakeOps {
        registers: StdMutex<Vec<(String, i32, Option<String>)>>,
        deregisters: StdMutex<Vec<(String, i32)>>,
        calls: StdMutex<Vec<&'static str>>,
        fail_register: StdMutex<bool>,
        fail_deregister: StdMutex<bool>,
        terminal_deregister: StdMutex<bool>,
        deregister_unhealthy: StdMutex<Option<String>>,
        deregister_rejection: StdMutex<Option<i32>>,
        block_register: StdMutex<bool>,
        register_started: Notify,
        register_release: Notify,
    }

    #[async_trait]
    impl EndpointOps for FakeOps {
        async fn register(
            &self,
            _namespace: &str,
            mcp_name: &str,
            _address: &str,
            port: i32,
            version: Option<&str>,
        ) -> Result<()> {
            let failed = *self.fail_register.lock().expect("lock");
            if failed {
                return Err(crate::api::error::Error::ErrResult("boom".to_string()));
            }
            self.calls.lock().expect("lock").push("register");
            let blocked = *self.block_register.lock().expect("lock");
            if blocked {
                self.register_started.notify_one();
                self.register_release.notified().await;
            }
            self.registers.lock().expect("lock").push((
                mcp_name.to_string(),
                port,
                version.map(str::to_string),
            ));
            Ok(())
        }

        async fn deregister(
            &self,
            _namespace: &str,
            mcp_name: &str,
            _address: &str,
            port: i32,
        ) -> Result<()> {
            let failed = *self.fail_deregister.lock().expect("lock");
            if failed {
                return Err(crate::api::error::Error::ErrResult("boom".to_string()));
            }
            if let Some(message) = self.deregister_unhealthy.lock().expect("lock").clone() {
                return Err(crate::api::error::Error::ClientUnhealthy(message));
            }
            let terminal = *self.terminal_deregister.lock().expect("lock");
            if terminal {
                return Err(crate::api::error::Error::NoAvailableServer);
            }
            let rejection = *self.deregister_rejection.lock().expect("lock");
            if let Some(code) = rejection {
                return Err(crate::api::error::Error::ErrResponse(
                    None,
                    500,
                    code,
                    Some("rejected".to_string()),
                ));
            }
            self.calls.lock().expect("lock").push("deregister");
            self.deregisters
                .lock()
                .expect("lock")
                .push((mcp_name.to_string(), port));
            Ok(())
        }
    }

    async fn connected_redo(ops: &Arc<FakeOps>) -> Arc<EndpointRedo> {
        let redo = EndpointRedo::new("public".to_string(), ops.clone(), Duration::from_secs(3600));
        redo.on_connected("conn-1".to_string()).await;
        redo
    }

    fn registers(ops: &FakeOps) -> usize {
        ops.registers.lock().expect("lock").len()
    }

    #[tokio::test]
    async fn test_register_then_reconnect_triggers_reregister() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
            .await
            .expect("register should succeed");
        assert_eq!(registers(&ops), 1);

        redo.on_connected("conn-2".to_string()).await;
        redo.flush().await;
        assert_eq!(registers(&ops), 2);
    }

    #[tokio::test]
    async fn test_deregister_failure_is_retried() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
            .await
            .expect("register should succeed");
        *ops.fail_deregister.lock().expect("lock") = true;
        assert!(
            redo.deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
                .await
                .is_err()
        );

        *ops.fail_deregister.lock().expect("lock") = false;
        redo.flush().await;
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 1);
    }

    #[tokio::test]
    async fn test_deregister_client_side_failures_are_retried() {
        for message in [
            error_messages::REQUEST_TIMEOUT,
            error_messages::REQUEST_QUEUE_FULL,
        ] {
            let ops = Arc::new(FakeOps::default());
            let redo = connected_redo(&ops).await;
            redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
                .await
                .expect("register should succeed");

            *ops.deregister_unhealthy.lock().expect("lock") = Some(message.to_string());
            assert!(
                redo.deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
                    .await
                    .is_err(),
                "{message} must remain retriable"
            );

            *ops.deregister_unhealthy.lock().expect("lock") = None;
            redo.flush().await;
            assert_eq!(ops.deregisters.lock().expect("lock").len(), 1);
            redo.flush().await;
            assert_eq!(ops.deregisters.lock().expect("lock").len(), 1);
        }
    }

    #[tokio::test]
    async fn test_disconnect_deregister_error_removes_entry() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
            .await
            .expect("register should succeed");

        *ops.terminal_deregister.lock().expect("lock") = true;
        redo.deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
            .await
            .expect("disconnect during deregister is terminal");

        redo.flush().await;
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 0);
        assert_eq!(registers(&ops), 1);
    }

    #[tokio::test]
    async fn test_permanent_deregister_rejection_is_terminal() {
        for code in [404, 400] {
            let ops = Arc::new(FakeOps::default());
            let redo = connected_redo(&ops).await;
            redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
                .await
                .expect("register should succeed");

            *ops.deregister_rejection.lock().expect("lock") = Some(code);
            redo.deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
                .await
                .unwrap_or_else(|error| {
                    panic!("a {code} rejection should be terminal, got {error}")
                });

            *ops.deregister_rejection.lock().expect("lock") = None;
            redo.flush().await;
            assert_eq!(ops.deregisters.lock().expect("lock").len(), 0);
            assert_eq!(registers(&ops), 1);
        }
    }

    #[tokio::test]
    async fn test_first_connection_clears_pending_deregister() {
        let ops = Arc::new(FakeOps::default());
        let redo = EndpointRedo::new("public".to_string(), ops.clone(), Duration::from_secs(3600));
        *ops.fail_deregister.lock().expect("lock") = true;
        assert!(
            redo.deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
                .await
                .is_err()
        );

        redo.on_connected("conn-1".to_string()).await;
        *ops.fail_deregister.lock().expect("lock") = false;
        redo.flush().await;
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 0);
    }

    #[tokio::test]
    async fn test_reconnect_clears_pending_deregister() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
            .await
            .expect("register should succeed");

        *ops.fail_deregister.lock().expect("lock") = true;
        assert!(
            redo.deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
                .await
                .is_err()
        );
        *ops.fail_deregister.lock().expect("lock") = false;

        redo.on_connected("conn-2".to_string()).await;
        redo.flush().await;
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 0);
    }

    #[test]
    fn test_disconnect_error_classification() {
        assert!(is_disconnect_error(
            &crate::api::error::Error::ClientUnhealthy("down".to_string())
        ));
        assert!(is_disconnect_error(
            &crate::api::error::Error::ClientUnhealthy(error_messages::CONNECTION_LOST.to_string())
        ));
        assert!(!is_disconnect_error(
            &crate::api::error::Error::ClientUnhealthy(error_messages::REQUEST_TIMEOUT.to_string())
        ));
        assert!(!is_disconnect_error(
            &crate::api::error::Error::ClientUnhealthy(
                error_messages::REQUEST_QUEUE_FULL.to_string()
            )
        ));
        assert!(is_disconnect_error(
            &crate::api::error::Error::NoAvailableServer
        ));
        assert!(is_disconnect_error(
            &crate::api::error::Error::TonicGrpcStatus(tonic::Status::unavailable("down"))
        ));
        assert!(is_disconnect_error(
            &crate::api::error::Error::TonicGrpcStatus(tonic::Status::cancelled("cancelled"))
        ));
        assert!(!is_disconnect_error(
            &crate::api::error::Error::TonicGrpcStatus(tonic::Status::internal("boom"))
        ));
        assert!(!is_disconnect_error(&crate::api::error::Error::ErrResult(
            "boom".to_string()
        )));
    }

    #[tokio::test]
    async fn test_user_deregister_waits_for_inflight_register() {
        let ops = Arc::new(FakeOps::default());
        *ops.block_register.lock().expect("lock") = true;
        let redo = connected_redo(&ops).await;

        let register_redo = redo.clone();
        let register_task = tokio::spawn(async move {
            register_redo
                .register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
                .await
        });
        ops.register_started.notified().await;

        let deregister_redo = redo.clone();
        let deregister_task = tokio::spawn(async move {
            deregister_redo
                .deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !deregister_task.is_finished(),
            "deregister must wait for the in-flight register"
        );
        assert!(ops.deregisters.lock().expect("lock").is_empty());
        assert_eq!(ops.calls.lock().expect("lock").as_slice(), ["register"]);

        ops.register_release.notify_one();
        register_task
            .await
            .expect("register task should join")
            .expect("register should succeed");
        deregister_task
            .await
            .expect("deregister task should join")
            .expect("deregister should succeed");

        assert_eq!(
            ops.calls.lock().expect("lock").as_slice(),
            ["register", "deregister"]
        );
        assert_eq!(registers(&ops), 1);
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 1);

        redo.flush().await;
        assert_eq!(registers(&ops), 1);
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 1);
    }

    #[tokio::test]
    async fn test_register_failure_stays_pending_and_retries() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        *ops.fail_register.lock().expect("lock") = true;
        assert!(
            redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
                .await
                .is_err()
        );
        assert_eq!(registers(&ops), 0);

        *ops.fail_register.lock().expect("lock") = false;
        redo.flush().await;
        assert_eq!(registers(&ops), 1);

        redo.flush().await;
        assert_eq!(registers(&ops), 1);
    }

    #[tokio::test]
    async fn test_flush_on_same_connection_does_not_reregister() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
            .await
            .expect("register should succeed");

        redo.flush().await;
        assert_eq!(registers(&ops), 1);
    }

    #[tokio::test]
    async fn test_reconnect_reregister_preserves_version() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        redo.register(
            "svc".to_string(),
            "127.0.0.1".to_string(),
            18080,
            Some("1.0.0".to_string()),
        )
        .await
        .expect("register should succeed");

        redo.on_connected("conn-2".to_string()).await;
        redo.flush().await;
        let seen = ops.registers.lock().expect("lock");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1].2.as_deref(), Some("1.0.0"));
    }

    #[tokio::test]
    async fn test_deregister_removes_entry_and_unknown_key_sends_once() {
        let ops = Arc::new(FakeOps::default());
        let redo = connected_redo(&ops).await;
        redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
            .await
            .expect("register should succeed");
        redo.deregister("svc".to_string(), "127.0.0.1".to_string(), 18080)
            .await
            .expect("deregister should succeed");
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 1);

        redo.flush().await;
        assert_eq!(ops.deregisters.lock().expect("lock").len(), 1);
        assert_eq!(registers(&ops), 1);

        let unknown_ops = Arc::new(FakeOps::default());
        let unknown_redo = EndpointRedo::new(
            "public".to_string(),
            unknown_ops.clone(),
            Duration::from_secs(3600),
        );
        unknown_redo
            .deregister("ghost".to_string(), "127.0.0.1".to_string(), 18081)
            .await
            .expect("unknown deregister should succeed");
        assert_eq!(unknown_ops.deregisters.lock().expect("lock").len(), 1);
    }

    #[tokio::test]
    async fn test_ticker_does_not_keep_redo_alive() {
        let ops = Arc::new(FakeOps::default());
        let redo = EndpointRedo::new("public".to_string(), ops, Duration::from_millis(20));
        redo.on_connected("conn-1".to_string()).await;
        redo.register("svc".to_string(), "127.0.0.1".to_string(), 18080, None)
            .await
            .expect("register should succeed");

        let weak = Arc::downgrade(&redo);
        drop(redo);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(weak.strong_count(), 0);
    }
}
