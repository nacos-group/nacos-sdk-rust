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
