use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use futures::Future;
use tokio::task::JoinHandle;

use crate::common::remote::grpc::message::GrpcMessageBuilder;
use crate::common::remote::grpc::message::request::SetupAckRequest;
use crate::common::remote::grpc::message::response::ServerCheckResponse;
use crate::nacos_proto::v2::Payload;

use super::Spawner;

struct TokioSpawner;

impl Spawner for TokioSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        tokio::spawn(future)
    }
}

pub(crate) fn test_spawner() -> Arc<dyn Spawner> {
    Arc::new(TokioSpawner)
}

pub(crate) fn server_check_payload(id: &str, support_ability_negotiation: bool) -> Payload {
    let response = ServerCheckResponse {
        connection_id: Some(id.to_string()),
        support_ability_negotiation,
        ..Default::default()
    };
    GrpcMessageBuilder::new(response)
        .build()
        .into_payload()
        .expect("server check payload")
}

pub(crate) fn setup_ack_payload(ability_table: HashMap<String, bool>) -> Payload {
    let ack = SetupAckRequest {
        ability_table: Some(ability_table),
        ..Default::default()
    };
    GrpcMessageBuilder::new(ack)
        .build()
        .into_payload()
        .expect("setup ack payload")
}
