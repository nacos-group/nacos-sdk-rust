use crate::common::remote::request::{Request, LOCAL_IP};
use crate::common::remote::response::server_response::ServerCheckServerResponse;
use crate::common::remote::response::{Response, TYPE_SERVER_CHECK_SERVER_RESPONSE};
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

pub(crate) fn build_server_response(
    resp_payload: Payload,
) -> crate::api::error::Result<Box<dyn Response>> {
    let metadata = resp_payload.metadata.unwrap();
    let body_data = resp_payload.body.unwrap().value;
    let type_url = metadata.r#type;
    if TYPE_SERVER_CHECK_SERVER_RESPONSE.eq(&type_url) {
        let de: ServerCheckServerResponse =
            serde_json::from_str(String::from_utf8(body_data).unwrap().as_str())?;
        return Ok(Box::new(de));
    }
    Err(crate::api::error::Error::Deserialization(type_url))
}
