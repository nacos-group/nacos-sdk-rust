mod cache;
mod handler;
mod message;
mod util;
mod worker;

use crate::api::config::ConfigService;
use crate::api::plugin::{AuthPlugin, ConfigFilter};
use crate::api::props::ClientProps;
use crate::config::worker::ConfigWorker;

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

impl ConfigService for NacosConfigService {
    fn get_config(
        &mut self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<crate::api::config::ConfigResponse> {
        self.client_worker.get_config(data_id, group)
    }

    fn publish_config(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> crate::api::error::Result<bool> {
        self.client_worker
            .publish_config(data_id, group, content, content_type)
    }

    fn publish_config_cas(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: String,
    ) -> crate::api::error::Result<bool> {
        self.client_worker
            .publish_config_cas(data_id, group, content, content_type, cas_md5)
    }

    fn publish_config_beta(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        beta_ips: String,
    ) -> crate::api::error::Result<bool> {
        self.client_worker
            .publish_config_beta(data_id, group, content, content_type, beta_ips)
    }

    fn publish_config_param(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: std::collections::HashMap<String, String>,
    ) -> crate::api::error::Result<bool> {
        self.client_worker.publish_config_param(
            data_id,
            group,
            content,
            content_type,
            cas_md5,
            params,
        )
    }

    fn remove_config(&mut self, data_id: String, group: String) -> crate::api::error::Result<bool> {
        self.client_worker.remove_config(data_id, group)
    }

    fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        self.client_worker.add_listener(data_id, group, listener);
        Ok(())
    }

    fn remove_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        self.client_worker.remove_listener(data_id, group, listener);
        Ok(())
    }
}
