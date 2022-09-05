use crate::common::remote::request::client_request::ConnectResetRequest;
use crate::common::remote::request::{Request, LOCAL_IP, TYPE_CONNECT_RESET_SERVER_REQUEST};
use crate::common::remote::response::server_response::ServerCheckServerResponse;
use crate::common::remote::response::{Response, TYPE_SERVER_CHECK_SERVER_RESPONSE};
use crate::nacos_proto::v2::{Metadata, Payload};
use serde::Serialize;

pub(crate) fn build_req_grpc_payload(req: impl Request + Serialize) -> Payload {
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

pub(crate) fn build_resp_grpc_payload(resp: impl Response + Serialize) -> Payload {
    let json_val = serde_json::to_vec(&resp).unwrap();
    let metadata = Metadata {
        r#type: resp.get_type_url().to_string(),
        client_ip: LOCAL_IP.clone(),
        headers: std::collections::HashMap::new(),
    };
    Payload {
        metadata: Some(metadata),
        body: Some(prost_types::Any {
            type_url: resp.get_type_url().to_string(),
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

pub(crate) fn build_server_request(
    req_payload: Payload,
) -> crate::api::error::Result<Box<dyn Request>> {
    let metadata = req_payload.metadata.unwrap();
    let body_data = req_payload.body.unwrap().value;
    let type_url = metadata.r#type;
    if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&type_url) {
        let de: ConnectResetRequest =
            serde_json::from_str(String::from_utf8(body_data).unwrap().as_str())?;
        return Ok(Box::new(de));
    }
    Err(crate::api::error::Error::Deserialization(type_url))
}

#[cfg(test)]
mod tests {
    use crate::common::remote::response::server_response::ServerCheckServerResponse;
    use crate::common::remote::response::Response;

    #[test]
    fn it_works_serde_json() {
        let data = r#"
        {
            "connectionId": "uuid",
            "requestId": "666",
            "resultCode": "SUCCESS",
            "errorCode": 0
        }"#;
        let resp: ServerCheckServerResponse = serde_json::from_str(data).unwrap();
        println!("serde_json resp {:?}", resp);
        assert_eq!(resp.get_request_id().as_str(), "666");
    }
}
