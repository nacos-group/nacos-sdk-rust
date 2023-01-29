use std::sync::Arc;

use tracing::{error, info};

use crate::common::{event_bus::NacosEventSubscriber, executor, remote::grpc::NacosServerSetUP};

use crate::common::remote::grpc::events::GrpcReconnectedEvent;
use crate::common::remote::grpc::NacosGrpcClient;

pub(crate) struct GrpcReconnectedEventSubscriber {
    pub(crate) nacos_grpc_client: Arc<NacosGrpcClient>,
    pub(crate) set_up_info: NacosServerSetUP,
}

impl NacosEventSubscriber for GrpcReconnectedEventSubscriber {
    type EventType = GrpcReconnectedEvent;

    fn on_event(&self, _: &Self::EventType) {
        info!("received grpc reconnect event.");

        let nacos_grpc_client = self.nacos_grpc_client.clone();
        let set_up_info = self.set_up_info.clone();

        executor::spawn(async move {
            let init_ret = nacos_grpc_client.init(set_up_info).await;
            if let Err(e) = init_ret {
                error!("grpc client reconnect failed, {:?}", e);
            }
        });
    }
}
