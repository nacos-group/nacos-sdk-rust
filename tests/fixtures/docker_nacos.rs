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

const DEFAULT_NACOS_IMAGE: &str = "nacos/nacos-server:v2.5.2";
const GRPC_PORT_OFFSET: u16 = 1000;

pub struct DockerNacosServer {
    http_port: u16,
    grpc_port: u16,
    container_name: String,
    container: Option<Child>,
    running: Arc<AtomicBool>,
    image: String,
}

impl DockerNacosServer {
    pub fn new(http_port: u16) -> Self {
        let grpc_port = http_port + GRPC_PORT_OFFSET;
        let container_name = format!("nacos-test-{}", http_port);
        let image =
            std::env::var("NACOS_DOCKER_IMAGE").unwrap_or_else(|_| DEFAULT_NACOS_IMAGE.to_string());

        Self {
            http_port,
            grpc_port,
            container_name,
            container: None,
            running: Arc::new(AtomicBool::new(false)),
            image,
        }
    }

    pub fn is_docker_available() -> bool {
        Command::new("docker")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn cleanup_existing(&self) -> Result<()> {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.container_name])
            .output();
        Ok(())
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
}

impl Drop for DockerNacosServer {
    fn drop(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            let _ = Command::new("docker")
                .args(["stop", &self.container_name])
                .output();
            let _ = Command::new("docker")
                .args(["rm", "-f", &self.container_name])
                .output();
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

impl DockerNacosServer {
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

        if !Self::is_docker_available() {
            return Err(FixtureError::Other(
                "Docker is not available on this system".to_string(),
            ));
        }

        self.cleanup_existing()?;

        let child = Command::new("docker")
            .args(["run", "-d", "--name", &self.container_name])
            .args(["-e", "MODE=standalone"])
            .args(["-p", &format!("{}:8848", self.http_port)])
            .args(["-p", &format!("{}:9848", self.grpc_port)])
            .arg(&self.image)
            .spawn()
            .map_err(|e| FixtureError::Other(format!("Failed to start Docker container: {e}")))?;

        self.container = Some(child);

        self.wait_for_healthy(DEFAULT_STARTUP_TIMEOUT_SECS).await
    }

    pub async fn stop(&mut self) -> Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        Command::new("docker")
            .args(["stop", &self.container_name])
            .output()
            .map_err(|e| FixtureError::Other(format!("Failed to stop Docker container: {e}")))?;

        Command::new("docker")
            .args(["rm", "-f", &self.container_name])
            .output()
            .map_err(|e| FixtureError::Other(format!("Failed to remove Docker container: {e}")))?;

        self.container = None;
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
            Ok(c) => c,
            Err(_) => return false,
        };

        match client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}
