use std::sync::Arc;

use tracing::{debug_span, error, info, Instrument};

use crate::common::{
    event_bus::NacosEventSubscriber,
    executor,
    remote::grpc::{NacosGrpcClient, NacosServerSetUP},
};

use crate::common::remote::grpc::events::ReconnectedEvent;

pub(crate) struct ReconnectedEventSubscriber {
    pub(crate) nacos_grpc_client: Arc<NacosGrpcClient>,
    pub(crate) set_up_info: NacosServerSetUP,
    pub(crate) scope: String,
}

impl NacosEventSubscriber for ReconnectedEventSubscriber {
    type EventType = ReconnectedEvent;

    fn on_event(&self, _: &Self::EventType) {
        let _reconnected_event_subscriber_span = debug_span!(
            parent: None,
            "reconnected_event_subscriber",
            client_id = self.scope
        )
        .entered();
        info!("received ReconnectedEvent.");

        let nacos_grpc_client = self.nacos_grpc_client.clone();
        let set_up_info = self.set_up_info.clone();

        executor::spawn(
            async move {
                let init_ret = nacos_grpc_client.init(set_up_info).await;
                if let Err(e) = init_ret {
                    error!("client reconnect failed, {e:?}");
                }
            }
            .in_current_span(),
        );
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}
