use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::api::error::Error;
use crate::common::remote::grpc::message::request::HealthCheckRequest;
use crate::common::remote::grpc::message::response::HealthCheckResponse;
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};

use super::config::ConnectionSettings;
use super::{ConnectionEvent, Transport, now_millis};

pub(crate) async fn run(
    transport: Arc<dyn Transport>,
    last_active: Arc<AtomicU64>,
    events_tx: mpsc::Sender<ConnectionEvent>,
    mut shutdown: watch::Receiver<bool>,
    settings: ConnectionSettings,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(settings.health_interval) => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
        }

        let idle = now_millis().saturating_sub(last_active.load(Ordering::Acquire));
        if idle < settings.health_interval.as_millis() as u64 {
            continue;
        }

        let mut healthy = false;
        let mut last_error: Option<Error> = None;
        for attempt in 0..settings.health_retry_times {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(rand::random_range(0..500))).await;
            }
            let request = match GrpcMessageBuilder::new(HealthCheckRequest::default())
                .build()
                .into_payload()
            {
                Ok(payload) => payload,
                Err(error) => {
                    last_error = Some(error);
                    break;
                }
            };
            match tokio::time::timeout(settings.health_timeout, transport.unary(request)).await {
                Ok(Ok(response)) => {
                    match GrpcMessage::<HealthCheckResponse>::from_payload(response) {
                        Ok(_) => {
                            healthy = true;
                            break;
                        }
                        Err(error) => {
                            warn!("health check response is invalid: {error}");
                            last_error = Some(error);
                        }
                    }
                }
                Ok(Err(error)) => {
                    warn!("health check request failed: {error}");
                    last_error = Some(error);
                }
                Err(_) => warn!("health check request timeout"),
            }
        }

        if healthy {
            debug!("health check passed");
            last_active.store(now_millis(), Ordering::Release);
        } else {
            let reason =
                last_error.unwrap_or_else(|| Error::ErrResult("health check failed".to_string()));
            let _ = events_tx.send(ConnectionEvent::Unhealthy(reason)).await;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::common::remote::grpc::nacos_grpc_service::GrpcStream;
    use crate::nacos_proto::v2::Payload;

    struct FailingTransport;

    #[async_trait]
    impl Transport for FailingTransport {
        async fn unary(&self, _payload: Payload) -> Result<Payload, Error> {
            Err(Error::ClientUnhealthy("health probe failed".to_string()))
        }

        async fn open_stream(
            &self,
            _request: GrpcStream<Payload>,
        ) -> Result<GrpcStream<Result<Payload, Error>>, Error> {
            Err(Error::ErrResult("open_stream is not used".to_string()))
        }
    }

    struct OkTransport;

    #[async_trait]
    impl Transport for OkTransport {
        async fn unary(&self, _payload: Payload) -> Result<Payload, Error> {
            GrpcMessageBuilder::new(HealthCheckResponse::default())
                .build()
                .into_payload()
        }

        async fn open_stream(
            &self,
            _request: GrpcStream<Payload>,
        ) -> Result<GrpcStream<Result<Payload, Error>>, Error> {
            Err(Error::ErrResult("open_stream is not used".to_string()))
        }
    }

    /// Health compares wall clock `now_millis()`, which paused time does not
    /// advance, so `last_active` is aged explicitly instead of via timers.
    fn aged_last_active() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(now_millis().saturating_sub(1000)))
    }

    fn health_settings() -> ConnectionSettings {
        ConnectionSettings {
            health_interval: Duration::from_millis(1),
            health_retry_times: 1,
            health_timeout: Duration::from_millis(10),
            ..Default::default()
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_unhealthy_transport_reports_and_stops() {
        let (events_tx, mut events_rx) = mpsc::channel(4);
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(run(
            Arc::new(FailingTransport),
            aged_last_active(),
            events_tx,
            shutdown_rx,
            health_settings(),
        ));

        let event = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .expect("the health failure must be reported")
            .expect("the event channel must stay open");
        assert!(
            matches!(event, ConnectionEvent::Unhealthy(_)),
            "unexpected event: {event:?}"
        );
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("health run must return after reporting")
            .expect("health task must not panic");
    }

    #[tokio::test(start_paused = true)]
    async fn test_healthy_transport_refreshes_activity_and_stops_on_shutdown() {
        let last_active = aged_last_active();
        let before = last_active.load(Ordering::Acquire);
        let (events_tx, mut events_rx) = mpsc::channel(4);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(run(
            Arc::new(OkTransport),
            last_active.clone(),
            events_tx,
            shutdown_rx,
            health_settings(),
        ));

        assert!(
            tokio::time::timeout(Duration::from_millis(50), events_rx.recv())
                .await
                .is_err(),
            "a healthy connection must not report Unhealthy"
        );
        assert!(
            last_active.load(Ordering::Acquire) > before,
            "a successful health check must refresh last_active"
        );

        shutdown_tx.send(true).expect("health run is still running");
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("health run must return on shutdown")
            .expect("health task must not panic");
    }
}
