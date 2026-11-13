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

//! Container-backed Nacos server fixture.
//!
//! Starts a Nacos standalone server in a container managed by either podman or
//! docker. The runtime is auto-detected (podman preferred, then docker) unless
//! `NACOS_CONTAINER_RUNTIME` explicitly selects one.

use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{DEFAULT_STARTUP_TIMEOUT_SECS, FixtureError, HEALTH_CHECK_INTERVAL_MS, Result};

const DEFAULT_NACOS_IMAGE: &str = "nacos/nacos-server:v2.5.4";
const GRPC_PORT_OFFSET: u16 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Podman,
    Docker,
}

impl ContainerRuntime {
    pub fn detect() -> Result<Self> {
        if let Ok(value) = std::env::var("NACOS_CONTAINER_RUNTIME") {
            return match value.as_str() {
                "podman" => Ok(Self::Podman),
                "docker" => Ok(Self::Docker),
                other => Err(FixtureError::Other(format!(
                    "unsupported NACOS_CONTAINER_RUNTIME={other}; expected podman or docker"
                ))),
            };
        }
        for candidate in [Self::Podman, Self::Docker] {
            if candidate.is_available() {
                return Ok(candidate);
            }
        }
        Err(FixtureError::Other(
            "neither podman nor docker is available".to_string(),
        ))
    }

    fn binary(self) -> &'static str {
        match self {
            Self::Podman => "podman",
            Self::Docker => "docker",
        }
    }

    fn is_available(self) -> bool {
        Command::new(self.binary())
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

pub struct ContainerNacosServer {
    http_port: u16,
    grpc_port: u16,
    container_name: String,
    runtime: Option<ContainerRuntime>,
    running: Arc<AtomicBool>,
    image: String,
}

impl ContainerNacosServer {
    pub fn new(http_port: u16) -> Self {
        let grpc_port = http_port + GRPC_PORT_OFFSET;
        let container_name = format!("nacos-test-{http_port}");
        let image =
            std::env::var("NACOS_DOCKER_IMAGE").unwrap_or_else(|_| DEFAULT_NACOS_IMAGE.to_string());
        Self {
            http_port,
            grpc_port,
            container_name,
            runtime: None,
            running: Arc::new(AtomicBool::new(false)),
            image,
        }
    }

    fn cleanup_existing(&self, runtime: ContainerRuntime) {
        let _ = Command::new(runtime.binary())
            .args(["rm", "-f", &self.container_name])
            .output();
    }

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
        let runtime = ContainerRuntime::detect()?;
        self.runtime = Some(runtime);
        self.cleanup_existing(runtime);
        Command::new(runtime.binary())
            .args(["run", "-d", "--name", &self.container_name])
            .args(["-e", "MODE=standalone"])
            .args(["-p", &format!("{}:8848", self.http_port)])
            .args(["-p", &format!("{}:9848", self.grpc_port)])
            .arg(&self.image)
            .spawn()
            .map_err(|e| {
                FixtureError::Other(format!(
                    "failed to start {} container: {e}",
                    runtime.binary()
                ))
            })?;
        self.wait_for_healthy(DEFAULT_STARTUP_TIMEOUT_SECS).await
    }

    pub async fn stop(&mut self) -> Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        let Some(runtime) = self.runtime else {
            return Ok(());
        };
        Command::new(runtime.binary())
            .args(["stop", &self.container_name])
            .output()
            .map_err(|e| FixtureError::Other(format!("failed to stop container: {e}")))?;
        Command::new(runtime.binary())
            .args(["rm", "-f", &self.container_name])
            .output()
            .map_err(|e| FixtureError::Other(format!("failed to remove container: {e}")))?;
        self.running.store(false, Ordering::SeqCst);
        Ok(())
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
            Ok(client) => client,
            Err(_) => return false,
        };
        match client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

impl Drop for ContainerNacosServer {
    fn drop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        if let Some(runtime) = self.runtime {
            let _ = Command::new(runtime.binary())
                .args(["stop", &self.container_name])
                .output();
            let _ = Command::new(runtime.binary())
                .args(["rm", "-f", &self.container_name])
                .output();
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_ports_and_name() {
        let server = ContainerNacosServer::new(18848);
        assert_eq!(server.server_addr(), "127.0.0.1:18848");
        assert_eq!(server.grpc_port(), 19848);
        assert_eq!(server.container_name, "nacos-test-18848");
    }

    #[test]
    fn test_runtime_binary_names() {
        assert_eq!(ContainerRuntime::Podman.binary(), "podman");
        assert_eq!(ContainerRuntime::Docker.binary(), "docker");
    }
}
