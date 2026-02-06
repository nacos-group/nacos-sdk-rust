//! Configuration management (config) implementation for the Nacos SDK.
//!
//! This module provides the internal implementation for configuration service
//! functionality, including:
//!
//! - `cache`: Local caching of configuration data with change notifications
//! - `handler`: Server push request handlers for configuration changes
//! - `message`: gRPC message definitions for config-related requests/responses
//! - `util`: Configuration-specific utilities (group key generation, parsing)
//! - `worker`: Background worker for config polling and change detection
//!
//! The public API for configuration management is exposed through [`crate::api::config::ConfigService`].

mod cache;
mod handler;
mod message;
mod util;
mod worker;

use std::sync::atomic::AtomicU64;

use tracing::instrument;

use crate::api::plugin::{AuthPlugin, ConfigFilter};
use crate::api::props::ClientProps;
use crate::config::worker::ConfigWorker;

pub(crate) struct NacosConfigService {
    /// config client worker
    client_worker: ConfigWorker,
    client_id: String,
}

impl std::fmt::Debug for NacosConfigService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NacosConfigService")
            .field(
                "namespace",
                &self.client_worker.client_props.get_namespace(),
            )
            .field("client_id", &self.client_id)
            .finish()
    }
}

const MODULE_NAME: &str = "config";
static SEQ: AtomicU64 = AtomicU64::new(1);

impl NacosConfigService {
    pub async fn new(
        client_props: ClientProps,
        auth_plugin: std::sync::Arc<dyn AuthPlugin>,
        config_filters: Vec<Box<dyn ConfigFilter>>,
    ) -> crate::api::error::Result<Self> {
        let client_id = crate::common::util::generate_client_id(
            MODULE_NAME,
            &client_props.get_server_addr(),
            &client_props.get_namespace(),
            &SEQ,
        );
        let client_worker =
            ConfigWorker::new(client_props, auth_plugin, config_filters, client_id.clone()).await?;
        Ok(Self {
            client_worker,
            client_id,
        })
    }
}

impl NacosConfigService {
    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn get_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<crate::api::config::ConfigResponse> {
        self.client_worker.get_config(data_id, group).await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn publish_config(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> crate::api::error::Result<bool> {
        self.client_worker
            .publish_config(data_id, group, content, content_type)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn publish_config_cas(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: String,
    ) -> crate::api::error::Result<bool> {
        self.client_worker
            .publish_config_cas(data_id, group, content, content_type, cas_md5)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn publish_config_beta(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        beta_ips: String,
    ) -> crate::api::error::Result<bool> {
        self.client_worker
            .publish_config_beta(data_id, group, content, content_type, beta_ips)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn publish_config_param(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: std::collections::HashMap<String, String>,
    ) -> crate::api::error::Result<bool> {
        self.client_worker
            .publish_config_param(data_id, group, content, content_type, cas_md5, params)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn remove_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<bool> {
        self.client_worker.remove_config(data_id, group).await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn add_listener(
        &self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        self.client_worker
            .add_listener(data_id, group, listener)
            .await;
        Ok(())
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    pub(crate) async fn remove_listener(
        &self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        self.client_worker
            .remove_listener(data_id, group, listener)
            .await;
        Ok(())
    }
}
