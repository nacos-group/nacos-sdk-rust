// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{DEFAULT_STARTUP_TIMEOUT_SECS, FixtureError, HEALTH_CHECK_INTERVAL_MS, Result};

/// gRPC port offset from HTTP port in rnacos.
const GRPC_PORT_OFFSET: u16 = 1000;

/// rnacos server instance that spawns a subprocess.
///
/// Manages the lifecycle of an `rnacos` binary process,
/// including startup, health checking, and cleanup.
pub struct RnacosServer {
    http_port: u16,
    grpc_port: u16,
    process: Option<Child>,
    running: Arc<AtomicBool>,
    binary_path: String,
}

impl RnacosServer {
    /// Creates a new RnacosServer with the specified HTTP port.
    ///
    /// The gRPC port is automatically set to `http_port + 1000`.
    ///
    /// # Arguments
    ///
    /// * `http_port` - HTTP port for the server.
    pub fn new(http_port: u16) -> Self {
        let grpc_port = http_port + GRPC_PORT_OFFSET;
        let binary_path = std::env::var("RNACOS_BINARY").unwrap_or_else(|_| "rnacos".to_string());

        Self {
            http_port,
            grpc_port,
            process: None,
            running: Arc::new(AtomicBool::new(false)),
            binary_path,
        }
    }

    /// Waits for the server to become healthy by polling the HTTP endpoint.
    async fn wait_for_healthy(&self, timeout_secs: u64) -> Result<()> {
        let start = tokio::time::Instant::now();
        let timeout = tokio::time::Duration::from_secs(timeout_secs);
        let interval = tokio::time::Duration::from_millis(HEALTH_CHECK_INTERVAL_MS);

        loop {
            if start.elapsed() > timeout {
                return Err(FixtureError::StartupTimeout);
            }

            if self.health_check().await {
                self.running.store(true, Ordering::SeqCst);
                return Ok(());
            }

            tokio::time::sleep(interval).await;
        }
    }
}

impl Drop for RnacosServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

impl RnacosServer {
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

        let child = Command::new(&self.binary_path)
            .env("RNACOS_HTTP_PORT", self.http_port.to_string())
            .env("RNACOS_GRPC_PORT", self.grpc_port.to_string())
            .spawn()
            .map_err(|e| {
                FixtureError::Other(format!(
                    "Failed to spawn rnacos binary '{}': {e}",
                    self.binary_path
                ))
            })?;

        self.process = Some(child);

        self.wait_for_healthy(DEFAULT_STARTUP_TIMEOUT_SECS).await
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            child
                .kill()
                .map_err(|e| FixtureError::Other(format!("Failed to kill rnacos process: {e}")))?;
            child.wait().map_err(|e| {
                FixtureError::Other(format!("Failed to wait for rnacos process: {e}"))
            })?;
        }
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub async fn health_check(&self) -> bool {
        let url = format!("http://127.0.0.1:{}", self.http_port);

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rnacos_server_ports() {
        let server = RnacosServer::new(18848);
        assert_eq!(server.http_port, 18848);
        assert_eq!(server.grpc_port, 19848);
    }

    #[test]
    fn test_rnacos_server_addr() {
        let server = RnacosServer::new(18848);
        assert_eq!(server.server_addr(), "127.0.0.1:18848");
    }
}
