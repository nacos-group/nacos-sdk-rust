use std::sync::Arc;

use rand::RngExt;

use super::ServerAddress;
use super::provider::ServerListProvider;

pub(crate) struct PollingServerListService {
    provider: Arc<dyn ServerListProvider>,
    index: usize,
}

impl PollingServerListService {
    #[cfg(test)]
    pub(crate) async fn new(server_list: Vec<String>) -> Self {
        let provider = Arc::new(super::provider::StaticServerListProvider::new(server_list));
        Self::from_provider(provider).await
    }

    pub(crate) async fn from_provider(provider: Arc<dyn ServerListProvider>) -> Self {
        let initial_list = provider.current_server_list().await;
        Self {
            provider,
            index: rand::rng().random_range(0..initial_list.len()),
        }
    }

    pub(crate) async fn select(&mut self) -> crate::api::error::Result<Arc<dyn ServerAddress>> {
        let server_list = self.provider.current_server_list().await;
        let parsed = super::parse_host_port_vec(&server_list)?;
        let idx = self.index % parsed.len();
        self.index = self.index.wrapping_add(1);
        let (host, port) = &parsed[idx];
        tracing::debug!("Polling server selected: {}:{} (index {})", host, port, idx);
        Ok(Arc::new(PollingServerAddress {
            host: host.to_string(),
            port: *port,
        }))
    }
}

struct PollingServerAddress {
    host: String,
    port: u16,
}

impl ServerAddress for PollingServerAddress {
    fn host(&self) -> &str {
        &self.host
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
pub mod tests {

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::PollingServerListService;
    use crate::common::remote::server_list::ServerListProvider;

    #[tokio::test]
    #[should_panic]
    pub async fn test_empty_server_list() {
        let _ = PollingServerListService::new(Vec::default()).await;
    }

    #[tokio::test]
    pub async fn test_illegal_format() {
        let mut service = PollingServerListService::new(vec!["127.0.0.1:sd".to_string()]).await;

        let result = service.select().await;
        assert!(result.is_err());
    }

    use crate::test_config::run_test;

    #[tokio::test]
    pub async fn test_poll_server_list() {
        run_test(|| async {
            let mut service = PollingServerListService::new(vec![
                "127.0.0.1:8848".to_string(),
                "127.0.0.2:8848".to_string(),
                "127.0.0.3:8848".to_string(),
            ])
            .await;

            let mut hosts = Vec::new();
            for _ in 0..6 {
                let server = service
                    .select()
                    .await
                    .expect("Failed to get server from service");
                hosts.push(server.host().to_string());
            }

            assert_eq!(hosts.len(), 6);
            for host in ["127.0.0.1", "127.0.0.2", "127.0.0.3"] {
                assert_eq!(
                    hosts.iter().filter(|entry| entry.as_str() == host).count(),
                    2,
                    "host {host} should be selected twice, got {hosts:?}"
                );
            }
        })
        .await;
    }

    struct RotatingProvider {
        calls: AtomicUsize,
    }

    impl RotatingProvider {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl ServerListProvider for RotatingProvider {
        async fn current_server_list(&self) -> Arc<Vec<String>> {
            let calls = self.calls.fetch_add(1, Ordering::SeqCst);
            if calls == 0 {
                Arc::new(vec!["127.0.0.1:8848".to_string()])
            } else {
                Arc::new(vec!["127.0.0.2:8848".to_string()])
            }
        }
    }

    #[tokio::test]
    async fn test_refresh_server_list_from_provider() {
        let provider = std::sync::Arc::new(RotatingProvider::new());
        let mut service = PollingServerListService::from_provider(provider).await;

        let server = service.select().await.expect("should get server");

        assert_eq!(server.host(), "127.0.0.2");
        assert_eq!(server.port(), 8848);
    }
}
