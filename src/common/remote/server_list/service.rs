use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use futures::Future;
use rand::RngExt;
use tower::Service;

use super::ServerAddress;
use super::provider::ServerListProvider;
use crate::api::error::Error;

pub(crate) struct PollingServerListService {
    provider: Arc<dyn ServerListProvider>,
    index: AtomicUsize,
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
            index: AtomicUsize::new(rand::rng().random_range(0..initial_list.len())),
        }
    }
}

impl Service<()> for PollingServerListService {
    type Response = Arc<dyn ServerAddress>;

    type Error = Error;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: ()) -> Self::Future {
        let provider = self.provider.clone();
        let next_idx = self.index.fetch_add(1, Ordering::Relaxed);

        Box::pin(async move {
            let server_list = provider.current_server_list().await;
            let parsed = super::parse_host_port(&server_list)?;
            let idx = next_idx % parsed.len();
            let (host, port) = &parsed[idx];
            let server_address = PollingServerAddress {
                host: host.clone(),
                port: *port,
            };
            Ok(Arc::new(server_address) as Arc<dyn ServerAddress>)
        })
    }
}

struct PollingServerAddress {
    host: String,
    port: u32,
}

impl ServerAddress for PollingServerAddress {
    fn host(&self) -> String {
        self.host.clone()
    }

    fn port(&self) -> u32 {
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

    use futures::future::poll_fn;
    use tower::Service;
    use tracing::debug;

    use crate::test_config;

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

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let result = service.call(()).await;
        assert!(result.is_err());
    }

    fn setup() {
        test_config::setup_log();
    }

    fn teardown() {}

    fn run_test<T, F>(test: F) -> T
    where
        F: FnOnce() -> T,
    {
        setup();
        let ret = test();
        teardown();
        ret
    }

    #[tokio::test]
    pub async fn test_poll_server_list() {
        run_test(|| async {
            let mut service = PollingServerListService::new(vec![
                "127.0.0.1:8848".to_string(),
                "127.0.0.2:8848".to_string(),
                "127.0.0.3:8848".to_string(),
            ])
            .await;

            let _ = poll_fn(|cx| service.poll_ready(cx)).await;
            let server1 = service
                .call(())
                .await
                .expect("Failed to get server from service");
            debug!("ip:{}, port:{}", server1.host(), server1.port());

            let _ = poll_fn(|cx| service.poll_ready(cx)).await;
            let server2 = service
                .call(())
                .await
                .expect("Failed to get server from service");
            debug!("ip:{}, port:{}", server2.host(), server2.port());

            let _ = poll_fn(|cx| service.poll_ready(cx)).await;
            let server3 = service
                .call(())
                .await
                .expect("Failed to get server from service");
            debug!("ip:{}, port:{}", server3.host(), server3.port());

            let _ = poll_fn(|cx| service.poll_ready(cx)).await;
            let server4 = service
                .call(())
                .await
                .expect("Failed to get server from service");
            debug!("ip:{}, port:{}", server4.host(), server4.port());

            let _ = poll_fn(|cx| service.poll_ready(cx)).await;
            let server5 = service
                .call(())
                .await
                .expect("Failed to get server from service");
            debug!("ip:{}, port:{}", server5.host(), server5.port());

            let _ = poll_fn(|cx| service.poll_ready(cx)).await;
            let server6 = service
                .call(())
                .await
                .expect("Failed to get server from service");
            debug!("ip:{}, port:{}", server6.host(), server6.port());

            let _ = poll_fn(|cx| service.poll_ready(cx)).await;
            let server7 = service
                .call(())
                .await
                .expect("Failed to get server from service");
            debug!("ip:{}, port:{}", server7.host(), server7.port());
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

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server = service.call(()).await.expect("should get server");

        assert_eq!(server.host(), "127.0.0.2");
        assert_eq!(server.port(), 8848);
    }
}
