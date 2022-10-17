mod client_request;
mod client_response;
mod server_request;
mod server_response;
mod util;
mod worker;

use crate::api::config::ConfigService;
use crate::api::props::ClientProps;
use crate::config::worker::ConfigWorker;

pub(crate) struct NacosConfigService {
    client_props: ClientProps,
    /// config client worker
    client_worker: ConfigWorker,
}

impl NacosConfigService {
    pub fn new(client_props: ClientProps) -> Self {
        let client_worker = ConfigWorker::new(client_props.clone());
        Self {
            client_props,
            client_worker,
        }
    }

    /// start Once
    pub(crate) async fn start(&mut self) {
        self.client_worker.start().await;
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

    fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        self.client_worker.add_listener(
            data_id,
            group,
            self.client_props.namespace.clone(),
            listener,
        );
        Ok(())
    }
}
