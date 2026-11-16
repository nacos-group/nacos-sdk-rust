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

use super::{FixtureError, HEALTH_CHECK_INTERVAL_MS, Result};

const DEFAULT_NACOS_IMAGE: &str = "nacos/nacos-server:v2.5.4";
const NACOS3_IMAGE: &str = "nacos/nacos-server:v3.3.0";
const NACOS3_STARTUP_TIMEOUT_SECS: u64 = 90;
const CONSOLE_PORT_OFFSET: u16 = 2000;
const GRPC_PORT_OFFSET: u16 = 1000;

/// Which server generation the fixture should start.
///
/// Nacos 3.x moved its readiness endpoint to the console (`/v3/console/health/readiness`
/// on port 8080) and enables authentication by default. This profile keeps those
/// differences out of the tests' general setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerProfile {
    Nacos2,
    Nacos3,
}

impl ServerProfile {
    pub fn from_env() -> Self {
        Self::parse(std::env::var("NACOS_SERVER_PROFILE").ok().as_deref())
    }

    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("3") | Some("3.x") | Some("3.3") | Some("3.3.0") => Self::Nacos3,
            Some("2") | Some("2.x") | Some("2.5.4") | None => Self::Nacos2,
            Some(other) => {
                eprintln!(
                    "unknown NACOS_SERVER_PROFILE={other}; falling back to the 2.5.4 profile"
                );
                Self::Nacos2
            }
        }
    }

    pub fn default_image(self) -> &'static str {
        match self {
            Self::Nacos2 => DEFAULT_NACOS_IMAGE,
            Self::Nacos3 => NACOS3_IMAGE,
        }
    }

    pub fn startup_timeout(self) -> std::time::Duration {
        std::time::Duration::from_secs(match self {
            Self::Nacos2 => super::DEFAULT_STARTUP_TIMEOUT_SECS,
            Self::Nacos3 => NACOS3_STARTUP_TIMEOUT_SECS,
        })
    }

    /// The host port mapped to the container's console port, when the profile
    /// needs one. The protocol test suites only use 8848/9848; the console is
    /// only needed for readiness.
    pub fn console_port(self, http_port: u16) -> Option<u16> {
        match self {
            Self::Nacos2 => None,
            Self::Nacos3 => Some(http_port + CONSOLE_PORT_OFFSET),
        }
    }

    /// Container environment for the profile.
    ///
    /// The 3.x image requires the auth token and server identity even when
    /// authentication is disabled; the values are test placeholders, not secrets.
    pub fn start_env(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Nacos2 => &[("MODE", "standalone")],
            Self::Nacos3 => &[
                ("MODE", "standalone"),
                ("NACOS_AUTH_ENABLE", "false"),
                ("NACOS_AUTH_ADMIN_ENABLE", "false"),
                ("NACOS_AUTH_CONSOLE_ENABLE", "false"),
                (
                    "NACOS_AUTH_TOKEN",
                    "VGhpc0lzTXlDdXN0b21TZWNyZXRLZXkwMTIzNDU2Nzg=",
                ),
                ("NACOS_AUTH_IDENTITY_KEY", "nacos-test"),
                ("NACOS_AUTH_IDENTITY_VALUE", "nacos-test"),
            ],
        }
    }

    /// Readiness URLs that must all be reachable. 3.x serves its official
    /// readiness on the console, but the console can report ready before the server
    /// API (8848/9848) is accepting traffic. The naming metrics endpoint is part
    /// of the server API, so both are required.
    pub fn readiness_urls(self, http_port: u16, console_port: Option<u16>) -> Vec<String> {
        match self {
            Self::Nacos2 => vec![format!(
                "http://127.0.0.1:{http_port}/nacos/v1/console/health/readiness"
            )],
            Self::Nacos3 => {
                let server_api =
                    format!("http://127.0.0.1:{http_port}/nacos/v1/ns/operator/metrics");
                match console_port {
                    Some(console_port) => vec![
                        format!("http://127.0.0.1:{console_port}/v3/console/health/readiness"),
                        server_api,
                    ],
                    None => vec![server_api],
                }
            }
        }
    }
}

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
    profile: ServerProfile,
}

impl ContainerNacosServer {
    pub fn new(http_port: u16) -> Self {
        Self::new_with_profile(http_port, ServerProfile::from_env())
    }

    pub fn new_with_profile(http_port: u16, profile: ServerProfile) -> Self {
        let grpc_port = http_port + GRPC_PORT_OFFSET;
        let container_name = format!("nacos-test-{http_port}");
        let image = std::env::var("NACOS_DOCKER_IMAGE")
            .unwrap_or_else(|_| profile.default_image().to_string());
        Self {
            http_port,
            grpc_port,
            container_name,
            runtime: None,
            running: Arc::new(AtomicBool::new(false)),
            image,
            profile,
        }
    }

    pub fn console_port(&self) -> Option<u16> {
        self.profile.console_port(self.http_port)
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

        let mut command = Command::new(runtime.binary());
        command.args(["run", "-d", "--name", &self.container_name]);
        for (key, value) in self.profile.start_env() {
            command.arg("-e").arg(format!("{key}={value}"));
        }
        command
            .args(["-p", &format!("{}:8848", self.http_port)])
            .args(["-p", &format!("{}:9848", self.grpc_port)]);
        if let Some(console_port) = self.console_port() {
            command.args(["-p", &format!("{console_port}:8080")]);
        }
        let output = command.arg(&self.image).output().map_err(|e| {
            FixtureError::Other(format!(
                "failed to start {} container: {e}",
                runtime.binary()
            ))
        })?;
        if !output.status.success() {
            // A failed `run` may still register a Created/stopped container.
            self.cleanup_existing(runtime);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FixtureError::Other(format!(
                "{} failed to start container {}: {}",
                runtime.binary(),
                self.container_name,
                stderr.trim()
            )));
        }
        self.wait_for_healthy(self.profile.startup_timeout().as_secs())
            .await
    }

    pub async fn stop(&mut self) -> Result<()> {
        // Best effort: `running` may be false while a failed `start()` still
        // left a Created container that must be removed.
        if let Some(runtime) = self.runtime {
            let _ = Command::new(runtime.binary())
                .args(["stop", &self.container_name])
                .output();
            let _ = Command::new(runtime.binary())
                .args(["rm", "-f", &self.container_name])
                .output();
        }
        self.running.store(false, Ordering::SeqCst);
        Ok(())
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
}

pub(super) async fn readiness_url_is_ok(url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(tokio::time::Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    match client.get(url).send().await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

impl Drop for ContainerNacosServer {
    fn drop(&mut self) {
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
    fn test_server_profile_parse() {
        assert_eq!(ServerProfile::parse(None), ServerProfile::Nacos2);
        assert_eq!(ServerProfile::parse(Some("2.5.4")), ServerProfile::Nacos2);
        assert_eq!(ServerProfile::parse(Some("3.3")), ServerProfile::Nacos3);
        assert_eq!(ServerProfile::parse(Some("3.3.0")), ServerProfile::Nacos3);
        assert_eq!(ServerProfile::parse(Some("unknown")), ServerProfile::Nacos2);
    }

    #[test]
    fn test_server_profile_ports_and_timeout() {
        assert_eq!(ServerProfile::Nacos2.console_port(8848), None);
        assert_eq!(ServerProfile::Nacos3.console_port(8848), Some(10848));
        assert!(ServerProfile::Nacos3.startup_timeout() > ServerProfile::Nacos2.startup_timeout());
    }

    #[test]
    fn test_server_profile_start_env() {
        assert!(
            !ServerProfile::Nacos2
                .start_env()
                .iter()
                .any(|(key, _)| *key == "NACOS_AUTH_TOKEN")
        );

        let env = ServerProfile::Nacos3.start_env();
        assert!(env.contains(&("MODE", "standalone")));
        assert!(env.contains(&("NACOS_AUTH_ENABLE", "false")));
        assert!(env.iter().any(|(key, _)| *key == "NACOS_AUTH_TOKEN"));
        assert!(env.iter().any(|(key, _)| *key == "NACOS_AUTH_IDENTITY_KEY"));
    }

    #[test]
    fn test_server_profile_readiness_urls() {
        assert_eq!(
            ServerProfile::Nacos2.readiness_urls(8848, None),
            vec!["http://127.0.0.1:8848/nacos/v1/console/health/readiness".to_string()]
        );

        assert_eq!(
            ServerProfile::Nacos3.readiness_urls(8848, Some(10848)),
            vec![
                "http://127.0.0.1:10848/v3/console/health/readiness".to_string(),
                "http://127.0.0.1:8848/nacos/v1/ns/operator/metrics".to_string(),
            ]
        );

        assert_eq!(
            ServerProfile::Nacos3.readiness_urls(8848, None),
            vec!["http://127.0.0.1:8848/nacos/v1/ns/operator/metrics".to_string()]
        );
    }

    #[test]
    fn test_container_ports_and_name() {
        let server = ContainerNacosServer::new_with_profile(18848, ServerProfile::Nacos2);
        assert_eq!(server.server_addr(), "127.0.0.1:18848");
        assert_eq!(server.grpc_port(), 19848);
        assert_eq!(server.container_name, "nacos-test-18848");
    }

    #[test]
    fn test_container_console_port_only_for_nacos3() {
        let nacos2 = ContainerNacosServer::new_with_profile(18848, ServerProfile::Nacos2);
        assert_eq!(nacos2.console_port(), None);

        let nacos3 = ContainerNacosServer::new_with_profile(18848, ServerProfile::Nacos3);
        assert_eq!(nacos3.console_port(), Some(20848));
    }

    #[test]
    fn test_runtime_binary_names() {
        assert_eq!(ContainerRuntime::Podman.binary(), "podman");
        assert_eq!(ContainerRuntime::Docker.binary(), "docker");
    }
}
