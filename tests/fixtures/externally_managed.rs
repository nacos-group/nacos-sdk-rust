use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{FixtureError, Result};

const GRPC_PORT_OFFSET: u16 = 1000;

pub struct ExternallyManagedServer {
    http_port: u16,
    grpc_port: u16,
    running: Arc<AtomicBool>,
}

impl ExternallyManagedServer {
    pub fn new(http_port: u16) -> Self {
        Self {
            http_port,
            grpc_port: http_port + GRPC_PORT_OFFSET,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn health_check(&self) -> bool {
        let url = format!(
            "http://127.0.0.1:{}/nacos/v1/console/health/readiness",
            self.http_port
        );
        let client = match reqwest::Client::builder()
            .timeout(tokio::time::Duration::from_secs(2))
            .build()
        {
            Ok(c) => c,
            Err(_) => return false,
        };

        match client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub fn server_addr(&self) -> String {
        format!("127.0.0.1:{}", self.http_port)
    }

    pub fn grpc_port(&self) -> u16 {
        self.grpc_port
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        if self.health_check().await {
            self.running.store(true, Ordering::SeqCst);
            return Ok(());
        }

        Err(FixtureError::Other(format!(
            "Externally managed Nacos not available on port {}",
            self.http_port
        )))
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }
}
