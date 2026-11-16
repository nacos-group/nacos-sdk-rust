//! Public AI API: MCP server registry, MCP subscriptions, endpoint publication,
//! and prompt queries.
//!
//! MCP requires Nacos Server >= 3.0.3, Prompt requires >= 3.2.0 (tested baseline
//! 3.3.0). `auth-by-aliyun` only signs naming and config resources, so AI
//! requests are currently sent unsigned (see `auth_by_aliyun_ram.rs`).
//!
//! Server abilities are read from the live connection. Ability checks wait for
//! a ready connection and read the table produced by the current handshake, so
//! a reconnect to a differently-versioned server is reflected without a
//! rebuild. The server remains the final authority for requests that race a
//! later disconnect.

use std::sync::Arc;
use std::time::Duration;

use crate::ai::NacosAiService;
use crate::api::error::{Error, Result};
use crate::api::plugin;
use crate::api::props::ClientProps;
use crate::common::util::check_not_blank;

pub use crate::ai::model::mcp::{
    McpEndpointInfo, McpEndpointSpec, McpResourceSpecification, McpServerBasicInfo,
    McpServerDetailInfo, McpServerRemoteServiceConfig, McpTool, McpToolAnnotations, McpToolMeta,
    McpToolSpecification, Repository, ServerVersionDetail,
};
pub use crate::ai::model::prompt::{Prompt, PromptVariable};
pub use crate::ai::updater::McpServerListener;

/// Default MCP subscription poll interval.
const DEFAULT_MCP_UPDATE_INTERVAL: Duration = Duration::from_secs(10);

/// Builder for [`AiService`].
pub struct AiServiceBuilder {
    client_props: ClientProps,
    auth_plugin: Option<Arc<dyn plugin::AuthPlugin>>,
    mcp_server_update_interval: Duration,
}

impl Default for AiServiceBuilder {
    fn default() -> Self {
        Self::new(ClientProps::new())
    }
}

impl AiServiceBuilder {
    pub fn new(client_props: ClientProps) -> Self {
        Self {
            client_props,
            auth_plugin: None,
            mcp_server_update_interval: DEFAULT_MCP_UPDATE_INTERVAL,
        }
    }

    #[cfg(feature = "auth-by-http")]
    pub fn enable_auth_plugin_http(self) -> Self {
        self.with_auth_plugin(Arc::new(plugin::HttpLoginAuthPlugin::default()))
    }

    /// Sets a custom [`plugin::AuthPlugin`] used to authenticate AI requests.
    pub fn with_auth_plugin(mut self, auth_plugin: Arc<dyn plugin::AuthPlugin>) -> Self {
        self.auth_plugin = Some(auth_plugin);
        self
    }

    /// Poll interval for MCP server subscriptions. Defaults to 10 seconds;
    /// values below 1 ms are clamped to 1 ms.
    pub fn mcp_server_update_interval(mut self, interval: Duration) -> Self {
        self.mcp_server_update_interval = interval.max(Duration::from_millis(1));
        self
    }

    /// Builds a new [`AiService`].
    pub async fn build(self) -> Result<AiService> {
        #[cfg(feature = "tracing-log")]
        crate::common::log::init_default();

        let auth_plugin = match self.auth_plugin {
            None => Arc::new(plugin::NoopAuthPlugin::default()),
            Some(plugin) => plugin,
        };
        let inner = NacosAiService::new(
            self.client_props,
            auth_plugin,
            self.mcp_server_update_interval,
        )
        .await?;
        Ok(AiService {
            inner: Arc::new(inner),
        })
    }
}

/// Nacos AI service.
///
/// Provides MCP server registry, MCP server subscriptions and prompt queries.
/// Cloneable so a single service can be shared across tasks.
#[derive(Clone)]
pub struct AiService {
    inner: Arc<NacosAiService>,
}

impl std::fmt::Debug for AiService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiService")
            .field("inner", &self.inner)
            .finish()
    }
}

impl AiService {
    /// Queries an MCP server by name, optionally pinned to `version`.
    pub async fn get_mcp_server(
        &self,
        mcp_name: impl Into<String>,
        version: Option<String>,
    ) -> Result<McpServerDetailInfo> {
        let mcp_name = check_not_blank(mcp_name, "mcp_name")?;
        self.inner.get_mcp_server(mcp_name, version).await
    }

    /// Publishes an MCP server as an active release and returns its id.
    pub async fn release_mcp_server(
        &self,
        basic: McpServerBasicInfo,
        tools: Option<McpToolSpecification>,
        resources: Option<McpResourceSpecification>,
        endpoint: Option<McpEndpointSpec>,
    ) -> Result<String> {
        self.release(basic, tools, resources, endpoint, false).await
    }

    /// Publishes an MCP server as a draft release and returns its id.
    ///
    /// Requires a server that supports MCP lifecycle draft releases.
    pub async fn release_mcp_server_draft(
        &self,
        basic: McpServerBasicInfo,
        tools: Option<McpToolSpecification>,
        resources: Option<McpResourceSpecification>,
        endpoint: Option<McpEndpointSpec>,
    ) -> Result<String> {
        self.release(basic, tools, resources, endpoint, true).await
    }

    async fn release(
        &self,
        basic: McpServerBasicInfo,
        tools: Option<McpToolSpecification>,
        resources: Option<McpResourceSpecification>,
        endpoint: Option<McpEndpointSpec>,
        create_draft: bool,
    ) -> Result<String> {
        let mcp_name = release_mcp_name(&basic)?;
        self.inner
            .release_mcp_server(basic, mcp_name, tools, resources, endpoint, create_draft)
            .await
    }

    /// Subscribes to an MCP server's changes, optionally pinned to `version`.
    ///
    /// The current detail is returned when it exists (`None` when the server is
    /// not found, in which case polling continues until it appears). Changes are
    /// delivered to `listener` in the background until unsubscribed.
    ///
    /// A cold subscription dispatches the first value when it routes to the
    /// subscribed key; a version-pinned subscription to the latest version may
    /// receive no initial callback. The returned value is always the authoritative
    /// initial state.
    ///
    /// Listener identity is compared with `Arc::ptr_eq`; pass the same `Arc`
    /// instance (or a clone of it) that was registered.
    pub async fn subscribe_mcp_server(
        &self,
        mcp_name: impl Into<String>,
        version: Option<String>,
        listener: Arc<dyn McpServerListener>,
    ) -> Result<Option<McpServerDetailInfo>> {
        let mcp_name = check_not_blank(mcp_name, "mcp_name")?;
        self.inner
            .subscribe_mcp_server(mcp_name, version, listener)
            .await
    }

    /// Removes `listener` from an MCP server subscription. The last listener
    /// stops the server's polling task.
    ///
    /// Listener identity is compared with `Arc::ptr_eq`; pass the same `Arc`
    /// instance (or a clone of it) that was registered.
    pub async fn unsubscribe_mcp_server(
        &self,
        mcp_name: impl Into<String>,
        version: Option<String>,
        listener: Arc<dyn McpServerListener>,
    ) -> Result<()> {
        let mcp_name = check_not_blank(mcp_name, "mcp_name")?;
        self.inner
            .unsubscribe_mcp_server(mcp_name, version, &listener)
            .await
    }

    /// Queries a prompt by key, optionally pinned to `version` or `label`.
    pub async fn get_prompt(
        &self,
        prompt_key: impl Into<String>,
        version: Option<String>,
        label: Option<String>,
    ) -> Result<Prompt> {
        let prompt_key = check_not_blank(prompt_key, "prompt_key")?;
        self.inner.get_prompt(prompt_key, version, label).await
    }

    /// Registers an MCP server endpoint with the registry.
    ///
    /// The endpoint is re-registered automatically after a reconnect, and a
    /// failed attempt is retried in the background.
    pub async fn register_mcp_server_endpoint(
        &self,
        mcp_name: impl Into<String>,
        address: impl Into<String>,
        port: u16,
        version: Option<String>,
    ) -> Result<()> {
        let mcp_name = check_not_blank(mcp_name, "mcp_name")?;
        let address = check_not_blank(address, "address")?;
        self.inner
            .register_mcp_server_endpoint(mcp_name, address, port, version)
            .await
    }

    /// Deregisters an MCP server endpoint from the registry.
    ///
    /// Transient failures (including disconnects) are retried in the background.
    /// A permanent server rejection (404/400) is terminal and resolves as success
    /// after one warning, since the endpoint is already absent server-side.
    pub async fn deregister_mcp_server_endpoint(
        &self,
        mcp_name: impl Into<String>,
        address: impl Into<String>,
        port: u16,
    ) -> Result<()> {
        let mcp_name = check_not_blank(mcp_name, "mcp_name")?;
        let address = check_not_blank(address, "address")?;
        self.inner
            .deregister_mcp_server_endpoint(mcp_name, address, port)
            .await
    }
}

fn release_mcp_name(basic: &McpServerBasicInfo) -> Result<String> {
    let name = basic.name.as_deref().ok_or_else(|| {
        Error::InvalidParam(
            "mcp_name".to_string(),
            "server specification name is required".to_string(),
        )
    })?;
    check_not_blank(name, "mcp_name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults_mcp_update_interval() {
        let builder = AiServiceBuilder::new(ClientProps::new());
        assert_eq!(
            builder.mcp_server_update_interval,
            DEFAULT_MCP_UPDATE_INTERVAL
        );
    }

    #[test]
    fn test_builder_overrides_mcp_update_interval() {
        let interval = Duration::from_secs(37);
        let builder =
            AiServiceBuilder::new(ClientProps::new()).mcp_server_update_interval(interval);
        assert_eq!(builder.mcp_server_update_interval, interval);
    }

    #[test]
    fn test_builder_clamps_zero_mcp_update_interval() {
        let builder =
            AiServiceBuilder::new(ClientProps::new()).mcp_server_update_interval(Duration::ZERO);
        assert_eq!(builder.mcp_server_update_interval, Duration::from_millis(1));
    }
}
