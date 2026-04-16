use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::time::Instant;

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
            return Arc::clone(&self.cached_server_list.load());
        }

        match fetch_server_list(&self.endpoint_url).await {
            Ok(server_list) => {
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
    if let Some(endpoint) = client_props.get_endpoint() {
        let namespace = client_props.get_namespace_default_if_empty();
        let endpoint_url = build_endpoint_url(&endpoint, &namespace);
        let initial_list = fetch_server_list(&endpoint_url).await?;
        let provider = EndpointServerListProvider::new(endpoint_url, initial_list);
        return Ok(Arc::new(provider));
    }

    let server_addr = client_props.get_server_addr();
    let server_list = super::parse_server_list(&server_addr)?;
    Ok(Arc::new(StaticServerListProvider::new(server_list)))
}

async fn fetch_server_list(endpoint_url: &str) -> crate::api::error::Result<Vec<String>> {
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

    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return append_namespace_if_missing(endpoint, namespace);
    }

    // Bare hostname — build URL with defaults
    let (host, explicit_port) = if let Some(colon_pos) = endpoint.find(':') {
        let (h, p) = endpoint.split_at(colon_pos);
        (h, Some(&p[1..]))
    } else {
        (endpoint, None)
    };

    let port = explicit_port
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_ENDPOINT_PORT);

    let url = format!("http://{}:{}{}", host, port, DEFAULT_ENDPOINT_URI);
    append_namespace_if_missing(&url, namespace)
}

fn append_namespace_if_missing(url: &str, namespace: &str) -> String {
    if namespace.trim().is_empty() {
        return url.to_string();
    }
    let query_part = url.split('?').nth(1).unwrap_or("");
    if query_contains_key(query_part, "namespace") {
        return url.to_string();
    }
    if url.contains('?') {
        format!("{}&namespace={}", url, namespace.trim())
    } else {
        format!("{}?namespace={}", url, namespace.trim())
    }
}

fn query_contains_key(query: &str, key: &str) -> bool {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .any(|part| part.split('=').next() == Some(key))
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
