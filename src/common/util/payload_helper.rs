use crate::common::remote::request::{Request, LOCAL_IP};
use crate::nacos_proto::v2::{Metadata, Payload};
use serde::Serialize;

pub(crate) fn build_grpc_payload(req: impl Request + Serialize) -> Payload {
    let json_val = serde_json::to_vec(&req).unwrap();
    let metadata = Metadata {
        r#type: req.get_type_url().to_string(),
        client_ip: LOCAL_IP.clone(),
        headers: req.get_headers().clone(),
    };
    Payload {
        metadata: Some(metadata),
        body: Some(prost_types::Any {
            type_url: req.get_type_url().to_string(),
            value: json_val,
        }),
    }
}
