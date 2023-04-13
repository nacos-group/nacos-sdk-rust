mod cache;
mod handler;
mod message;
mod util;
mod worker;

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::instrument;

use crate::api::config::ConfigService;
use crate::api::plugin::{AuthPlugin, ConfigFilter};
use crate::api::props::ClientProps;
use crate::config::worker::ConfigWorker;

pub(crate) struct NacosConfigService {
    /// config client worker
    client_worker: ConfigWorker,
    client_id: String,
}

const MODULE_NAME: &str = "config";
static SEQ: AtomicU64 = AtomicU64::new(1);

fn generate_client_id(server_addr: &str, namespace: &str) -> String {
    // module_name + server_addr + namespace + [seq]
    let client_id = format!(
        "{MODULE_NAME}:{server_addr}:{namespace}:{}",
        SEQ.fetch_add(1, Ordering::SeqCst)
    );
    client_id
}

impl NacosConfigService {
    pub fn new(
        client_props: ClientProps,
        auth_plugin: std::sync::Arc<dyn AuthPlugin>,
        config_filters: Vec<Box<dyn ConfigFilter>>,
    ) -> crate::api::error::Result<Self> {
        let client_id = generate_client_id(&client_props.server_addr, &client_props.namespace);
        let client_worker =
            ConfigWorker::new(client_props, auth_plugin, config_filters, client_id.clone())?;
        Ok(Self {
            client_worker,
            client_id,
        })
    }
}

#[cfg(not(feature = "async"))]
impl ConfigService for NacosConfigService {
    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn get_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<crate::api::config::ConfigResponse> {
        let future = self.client_worker.get_config(data_id, group);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn publish_config(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> crate::api::error::Result<bool> {
        let future = self
            .client_worker
            .publish_config(data_id, group, content, content_type);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn publish_config_cas(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: String,
    ) -> crate::api::error::Result<bool> {
        let future =
            self.client_worker
                .publish_config_cas(data_id, group, content, content_type, cas_md5);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn publish_config_beta(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        beta_ips: String,
    ) -> crate::api::error::Result<bool> {
        let future =
            self.client_worker
                .publish_config_beta(data_id, group, content, content_type, beta_ips);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn publish_config_param(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: std::collections::HashMap<String, String>,
    ) -> crate::api::error::Result<bool> {
        let future = self.client_worker.publish_config_param(
            data_id,
            group,
            content,
            content_type,
            cas_md5,
            params,
        );
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn remove_config(&self, data_id: String, group: String) -> crate::api::error::Result<bool> {
        let future = self.client_worker.remove_config(data_id, group);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn add_listener(
        &self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        let future = self.client_worker.add_listener(data_id, group, listener);
        futures::executor::block_on(future);
        Ok(())
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    fn remove_listener(
        &self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        let future = self.client_worker.remove_listener(data_id, group, listener);
        futures::executor::block_on(future);
        Ok(())
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl ConfigService for NacosConfigService {
    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    async fn get_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<crate::api::config::ConfigResponse> {
        self.client_worker.get_config(data_id, group).await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    async fn publish_config(
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
    async fn publish_config_cas(
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
    async fn publish_config_beta(
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
    async fn publish_config_param(
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
    async fn remove_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<bool> {
        self.client_worker.remove_config(data_id, group).await
    }

    #[instrument(fields(client_id = &self.client_id, group = group, data_id = data_id), skip_all)]
    async fn add_listener(
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
    async fn remove_listener(
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
