use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::container_nacos::{ServerProfile, readiness_url_is_ok};
use super::{FixtureError, Result};

const GRPC_PORT_OFFSET: u16 = 1000;

pub struct ExternallyManagedServer {
    http_port: u16,
    grpc_port: u16,
    running: Arc<AtomicBool>,
    profile: ServerProfile,
}

impl ExternallyManagedServer {
    pub fn new(http_port: u16) -> Self {
        Self {
            http_port,
            grpc_port: http_port + GRPC_PORT_OFFSET,
            running: Arc::new(AtomicBool::new(false)),
            profile: ServerProfile::from_env(),
        }
    }

    fn console_port(&self) -> Option<u16> {
        std::env::var("NACOS_CONSOLE_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .or_else(|| self.profile.console_port(self.http_port))
    }

    pub async fn health_check(&self) -> bool {
        for url in self
            .profile
            .readiness_urls(self.http_port, self.console_port())
        {
            if !readiness_url_is_ok(&url).await {
                return false;
            }
        }
        true
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
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
