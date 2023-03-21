use std::sync::Arc;

use tracing::{error, info};

use crate::common::{
    event_bus::NacosEventSubscriber,
    executor,
    remote::grpc::{NacosGrpcClient, NacosServerSetUP},
};

use crate::common::remote::grpc::events::ReconnectedEvent;

pub(crate) struct ReconnectedEventSubscriber {
    pub(crate) nacos_grpc_client: Arc<NacosGrpcClient>,
    pub(crate) set_up_info: NacosServerSetUP,
}

impl NacosEventSubscriber for ReconnectedEventSubscriber {
    type EventType = ReconnectedEvent;

    fn on_event(&self, _: &Self::EventType) {
        info!("received reconnect event.");

        let nacos_grpc_client = self.nacos_grpc_client.clone();
        let set_up_info = self.set_up_info.clone();

        executor::spawn(async move {
            let init_ret = nacos_grpc_client.init(set_up_info).await;
            if let Err(e) = init_ret {
                error!("client reconnect failed, {:?}", e);
            }
        });
    }
}
