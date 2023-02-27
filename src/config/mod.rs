mod cache;
mod handler;
mod message;
mod util;
mod worker;

use crate::api::config::ConfigService;
use crate::api::plugin::{AuthPlugin, ConfigFilter};
use crate::api::props::ClientProps;
use crate::config::worker::ConfigWorker;

#[cfg(feature = "async")]
use async_trait::async_trait;

pub(crate) struct NacosConfigService {
    /// config client worker
    client_worker: ConfigWorker,
}

impl NacosConfigService {
    pub fn new(
        client_props: ClientProps,
        auth_plugin: std::sync::Arc<dyn AuthPlugin>,
        config_filters: Vec<Box<dyn ConfigFilter>>,
    ) -> crate::api::error::Result<Self> {
        let client_worker = ConfigWorker::new(client_props, auth_plugin, config_filters)?;
        Ok(Self { client_worker })
    }
}

#[cfg(not(feature = "async"))]
impl ConfigService for NacosConfigService {
    fn get_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<crate::api::config::ConfigResponse> {
        let future = self.client_worker.get_config(data_id, group);
        futures::executor::block_on(future)
    }

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

    fn remove_config(&self, data_id: String, group: String) -> crate::api::error::Result<bool> {
        let future = self.client_worker.remove_config(data_id, group);
        futures::executor::block_on(future)
    }

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
#[async_trait]
impl ConfigService for NacosConfigService {
    async fn get_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<crate::api::config::ConfigResponse> {
        self.client_worker.get_config(data_id, group).await
    }

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

    async fn remove_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<bool> {
        self.client_worker.remove_config(data_id, group).await
    }

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
