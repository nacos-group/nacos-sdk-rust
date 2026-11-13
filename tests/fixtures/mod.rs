pub mod container_nacos;
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
    Container,
    ExternallyManaged,
    Rnacos,
}

impl Default for ServerMode {
    fn default() -> Self {
        parse_server_mode(std::env::var("NACOS_SERVER").ok().as_deref())
    }
}

pub(crate) fn parse_server_mode(value: Option<&str>) -> ServerMode {
    match value {
        Some("external") => ServerMode::ExternallyManaged,
        Some("rnacos") => ServerMode::Rnacos,
        Some("container") | Some("docker") | Some("podman") | None => ServerMode::Container,
        Some(other) => {
            eprintln!("unknown NACOS_SERVER={other}; falling back to container mode");
            ServerMode::Container
        }
    }
}

pub enum NacosServerEnum {
    Container(container_nacos::ContainerNacosServer),
    ExternallyManaged(externally_managed::ExternallyManagedServer),
    Rnacos(rnacos::RnacosServer),
}

impl NacosServerEnum {
    pub fn server_addr(&self) -> String {
        match self {
            Self::Container(s) => s.server_addr(),
            Self::ExternallyManaged(s) => s.server_addr(),
            Self::Rnacos(s) => s.server_addr(),
        }
    }

    pub fn grpc_port(&self) -> u16 {
        match self {
            Self::Container(s) => s.grpc_port(),
            Self::ExternallyManaged(s) => s.grpc_port(),
            Self::Rnacos(s) => s.grpc_port(),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        match self {
            Self::Container(s) => s.start().await,
            Self::ExternallyManaged(s) => s.start().await,
            Self::Rnacos(s) => s.start().await,
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        match self {
            Self::Container(s) => s.stop().await,
            Self::ExternallyManaged(s) => s.stop().await,
            Self::Rnacos(s) => s.stop().await,
        }
    }

    pub async fn health_check(&self) -> bool {
        match self {
            Self::Container(s) => s.health_check().await,
            Self::ExternallyManaged(s) => s.health_check().await,
            Self::Rnacos(s) => s.health_check().await,
        }
    }
}

pub fn create_server(mode: ServerMode, http_port: u16) -> NacosServerEnum {
    match mode {
        ServerMode::Container => {
            NacosServerEnum::Container(container_nacos::ContainerNacosServer::new(http_port))
        }
        ServerMode::ExternallyManaged => NacosServerEnum::ExternallyManaged(
            externally_managed::ExternallyManagedServer::new(DEFAULT_DOCKER_HTTP_PORT),
        ),
        ServerMode::Rnacos => NacosServerEnum::Rnacos(rnacos::RnacosServer::new(http_port)),
    }
}

pub const DEFAULT_DOCKER_HTTP_PORT: u16 = 8848;
pub const DEFAULT_STARTUP_TIMEOUT_SECS: u64 = 30;
pub const HEALTH_CHECK_INTERVAL_MS: u64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_server_mode_defaults_to_container() {
        assert_eq!(parse_server_mode(None), ServerMode::Container);
        assert_eq!(parse_server_mode(Some("container")), ServerMode::Container);
        assert_eq!(parse_server_mode(Some("docker")), ServerMode::Container);
        assert_eq!(parse_server_mode(Some("podman")), ServerMode::Container);
    }

    #[test]
    fn test_parse_server_mode_external_and_rnacos() {
        assert_eq!(
            parse_server_mode(Some("external")),
            ServerMode::ExternallyManaged
        );
        assert_eq!(parse_server_mode(Some("rnacos")), ServerMode::Rnacos);
    }

    #[test]
    fn test_parse_server_mode_unknown_falls_back_to_container() {
        assert_eq!(parse_server_mode(Some("unknown")), ServerMode::Container);
    }
}
