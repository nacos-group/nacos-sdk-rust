use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::time::Instant;
use url::Url;

use crate::api::error::Error;
use crate::api::props::ClientProps;

#[async_trait::async_trait]
pub trait ServerListProvider: Send + Sync {
    async fn current_server_list(&self) -> Arc<Vec<String>>;
}

pub(crate) struct StaticServerListProvider {
    server_list: Arc<Vec<String>>,
}

impl StaticServerListProvider {
    pub(crate) fn new(server_list: Vec<String>) -> Self {
        Self {
            server_list: Arc::new(server_list),
        }
    }
}

#[async_trait::async_trait]
impl ServerListProvider for StaticServerListProvider {
    async fn current_server_list(&self) -> Arc<Vec<String>> {
        Arc::clone(&self.server_list)
    }
}

pub(crate) struct EndpointServerListProvider {
    endpoint_url: String,
    refresh_interval: Duration,
    cached_server_list: ArcSwap<Vec<String>>,
    last_refresh_at: ArcSwap<Instant>,
}

impl EndpointServerListProvider {
    pub(crate) fn new(endpoint_url: String, initial_server_list: Vec<String>) -> Self {
        Self::with_refresh_interval(endpoint_url, initial_server_list, Duration::from_secs(30))
    }

    pub(crate) fn with_refresh_interval(
        endpoint_url: String,
        initial_server_list: Vec<String>,
        refresh_interval: Duration,
    ) -> Self {
        Self {
            endpoint_url,
            refresh_interval,
            cached_server_list: ArcSwap::from_pointee(initial_server_list),
            last_refresh_at: ArcSwap::from_pointee(Instant::now()),
        }
    }

    fn should_refresh(&self) -> bool {
        self.last_refresh_at.load().elapsed() >= self.refresh_interval
    }
}

#[async_trait::async_trait]
impl ServerListProvider for EndpointServerListProvider {
    async fn current_server_list(&self) -> Arc<Vec<String>> {
        if !self.should_refresh() {
            tracing::debug!(
                "Server list not yet due for refresh from endpoint {}, returning cached",
                self.endpoint_url
            );
            return Arc::clone(&self.cached_server_list.load());
        }

        match fetch_server_list(&self.endpoint_url).await {
            Ok(server_list) => {
                let cached = self.cached_server_list.load();
                let changed = !crate::common::util::unordered_eq(&cached, &server_list);
                if changed {
                    tracing::info!(
                        "Server list refreshed from endpoint {}: {:?} -> {:?}",
                        self.endpoint_url,
                        *cached,
                        server_list
                    );
                }
                self.cached_server_list.store(Arc::new(server_list));
            }
            Err(e) => {
                tracing::error!(
                    "Failed to refresh server list from endpoint {}: {}",
                    self.endpoint_url,
                    e
                );
            }
        }
        self.last_refresh_at.store(Arc::new(Instant::now()));

        Arc::clone(&self.cached_server_list.load())
    }
}

pub(crate) async fn create_server_list_provider(
    client_props: &ClientProps,
) -> crate::api::error::Result<Arc<dyn ServerListProvider>> {
    // The priority of the `endpoint` is higher than `server_addr`
    if let Some(endpoint) = client_props.get_endpoint() {
        let namespace = client_props.get_namespace_default_if_empty();
        let endpoint_url = build_endpoint_url(&endpoint, &namespace);
        let initial_list = fetch_server_list(&endpoint_url).await?;
        tracing::info!(
            "Create EndpointServerListProvider with server_list={:?}, endpoint_url={}",
            initial_list,
            endpoint_url
        );
        let provider = EndpointServerListProvider::new(endpoint_url, initial_list);
        Ok(Arc::new(provider))
    } else {
        let server_addr = client_props.get_server_addr();
        let server_list = super::parse_server_list(&server_addr)?;
        tracing::info!(
            "Create StaticServerListProvider with server_list={:?}",
            server_list
        );
        Ok(Arc::new(StaticServerListProvider::new(server_list)))
    }
}

async fn fetch_server_list(endpoint_url: &str) -> crate::api::error::Result<Vec<String>> {
    tracing::debug!("Fetching server list from {}", endpoint_url);
    let http_client = crate::common::remote::http_client();
    let response = http_client.get(endpoint_url).send().await.map_err(|err| {
        Error::WrongServerAddress(format!(
            "Failed to fetch endpoint server list from {}: {}",
            endpoint_url, err
        ))
    })?;

    let response = response.error_for_status().map_err(|err| {
        Error::WrongServerAddress(format!(
            "Failed to fetch endpoint server list from {}: {}",
            endpoint_url, err
        ))
    })?;

    let body = response.text().await.map_err(|err| {
        Error::WrongServerAddress(format!(
            "Failed to read endpoint server list from {}: {}",
            endpoint_url, err
        ))
    })?;

    super::parse_server_list(&body)
}

pub(crate) fn build_endpoint_url(endpoint: &str, namespace: &str) -> String {
    const DEFAULT_ENDPOINT_PORT: u16 = 8080;
    const DEFAULT_ENDPOINT_URI: &str = "/nacos/serverlist";

    let endpoint = endpoint.trim();
    let mut url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        Url::parse(endpoint).expect("endpoint URL should be valid")
    } else {
        // Bare hostname — parse host and optional port, then build URL with defaults
        let (host, port) = super::parse_host_port(endpoint);
        let port = port.unwrap_or(DEFAULT_ENDPOINT_PORT);
        Url::parse(&format!("http://{}:{}{}", host, port, DEFAULT_ENDPOINT_URI))
            .expect("constructed endpoint URL should be valid")
    };

    let namespace = namespace.trim();
    if !namespace.is_empty() && !url.query_pairs().any(|(k, _)| k == "namespace") {
        url.query_pairs_mut().append_pair("namespace", namespace);
    }

    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_endpoint_provider_returns_cached_on_refresh_failure() {
        let provider = EndpointServerListProvider {
            endpoint_url: "http://127.0.0.1:1/nacos/serverlist".to_string(),
            refresh_interval: Duration::from_secs(0),
            cached_server_list: ArcSwap::from_pointee(vec!["127.0.0.1:8848".to_string()]),
            last_refresh_at: ArcSwap::from_pointee(Instant::now() - Duration::from_secs(60)),
        };

        let result = provider.current_server_list().await;
        assert_eq!(&*result, &vec!["127.0.0.1:8848".to_string()]);
    }

    #[test]
    fn test_build_url_bare_hostname() {
        let url = build_endpoint_url("endpoint.example.com", "public");
        assert_eq!(
            url,
            "http://endpoint.example.com:8080/nacos/serverlist?namespace=public"
        );
    }

    #[test]
    fn test_build_url_bare_hostname_with_port() {
        let url = build_endpoint_url("endpoint.example.com:9090", "public");
        assert_eq!(
            url,
            "http://endpoint.example.com:9090/nacos/serverlist?namespace=public"
        );
    }

    #[test]
    fn test_build_url_full_http_url() {
        let url = build_endpoint_url(
            "http://endpoint.example.com:9090/custom/serverlist",
            "public",
        );
        assert_eq!(
            url,
            "http://endpoint.example.com:9090/custom/serverlist?namespace=public"
        );
    }

    #[test]
    fn test_build_url_full_https_url() {
        let url = build_endpoint_url("https://endpoint.example.com/custom/path?foo=bar", "public");
        assert_eq!(
            url,
            "https://endpoint.example.com/custom/path?foo=bar&namespace=public"
        );
    }

    #[test]
    fn test_build_url_full_url_with_existing_namespace() {
        let url = build_endpoint_url(
            "http://endpoint.example.com:8080/nacos/serverlist?namespace=existing",
            "public",
        );
        assert_eq!(
            url,
            "http://endpoint.example.com:8080/nacos/serverlist?namespace=existing"
        );
    }

    #[test]
    fn test_build_url_empty_namespace() {
        let url = build_endpoint_url("endpoint.example.com", "");
        assert_eq!(url, "http://endpoint.example.com:8080/nacos/serverlist");
    }
}
