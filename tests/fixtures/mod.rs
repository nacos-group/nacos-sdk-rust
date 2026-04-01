pub mod docker_nacos;
pub mod externally_managed;
pub mod rnacos;
pub mod shared_server;

use std::fmt;

pub type Result<T> = std::result::Result<T, FixtureError>;

#[derive(Debug)]
pub enum FixtureError {
    StartupTimeout,
    StopFailed,
    Io(std::io::Error),
    Other(String),
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixtureError::StartupTimeout => write!(f, "server failed to start within timeout"),
            FixtureError::StopFailed => write!(f, "server failed to stop gracefully"),
            FixtureError::Io(e) => write!(f, "I/O error: {e}"),
            FixtureError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FixtureError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FixtureError {
    fn from(e: std::io::Error) -> Self {
        FixtureError::Io(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMode {
    Rnacos,
    Docker,
    ExternallyManaged,
}

impl Default for ServerMode {
    fn default() -> Self {
        match std::env::var("NACOS_SERVER").as_deref() {
            Ok("docker") => ServerMode::Docker,
            Ok("external") => ServerMode::ExternallyManaged,
            _ => ServerMode::Rnacos,
        }
    }
}

pub enum NacosServerEnum {
    Rnacos(rnacos::RnacosServer),
    Docker(docker_nacos::DockerNacosServer),
    ExternallyManaged(externally_managed::ExternallyManagedServer),
}

impl NacosServerEnum {
    pub fn server_addr(&self) -> String {
        match self {
            NacosServerEnum::Rnacos(s) => s.server_addr(),
            NacosServerEnum::Docker(s) => s.server_addr(),
            NacosServerEnum::ExternallyManaged(s) => s.server_addr(),
        }
    }

    pub fn grpc_port(&self) -> u16 {
        match self {
            NacosServerEnum::Rnacos(s) => s.grpc_port(),
            NacosServerEnum::Docker(s) => s.grpc_port(),
            NacosServerEnum::ExternallyManaged(s) => s.grpc_port(),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        match self {
            NacosServerEnum::Rnacos(s) => s.start().await,
            NacosServerEnum::Docker(s) => s.start().await,
            NacosServerEnum::ExternallyManaged(s) => s.start().await,
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        match self {
            NacosServerEnum::Rnacos(s) => s.stop().await,
            NacosServerEnum::Docker(s) => s.stop().await,
            NacosServerEnum::ExternallyManaged(s) => s.stop().await,
        }
    }

    pub async fn health_check(&self) -> bool {
        match self {
            NacosServerEnum::Rnacos(s) => s.health_check().await,
            NacosServerEnum::Docker(s) => s.health_check().await,
            NacosServerEnum::ExternallyManaged(s) => s.health_check().await,
        }
    }
}

pub fn create_server(mode: ServerMode, http_port: u16) -> NacosServerEnum {
    match mode {
        ServerMode::Rnacos => NacosServerEnum::Rnacos(rnacos::RnacosServer::new(http_port)),
        ServerMode::Docker => {
            NacosServerEnum::Docker(docker_nacos::DockerNacosServer::new(http_port))
        }
        ServerMode::ExternallyManaged => NacosServerEnum::ExternallyManaged(
            externally_managed::ExternallyManagedServer::new(DEFAULT_DOCKER_HTTP_PORT),
        ),
    }
}

pub const DEFAULT_RNACOS_HTTP_PORT: u16 = 18848;
pub const DEFAULT_DOCKER_HTTP_PORT: u16 = 8848;
pub const DEFAULT_STARTUP_TIMEOUT_SECS: u64 = 30;
pub const HEALTH_CHECK_INTERVAL_MS: u64 = 500;
