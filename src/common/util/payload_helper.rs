use crate::common::remote::request::server_request::*;
use crate::common::remote::request::*;
use crate::common::remote::response::server_response::*;
use crate::common::remote::response::*;
use crate::nacos_proto::v2::{Metadata, Payload};
use serde::Serialize;
use std::collections::HashMap;

/// Payload Inner Model
pub(crate) struct PayloadInner {
    /// The data type of `body_str`
    pub(crate) type_url: String,
    /// Headers
    pub(crate) headers: HashMap<String, String>,
    /// json data
    pub(crate) body_str: String,
}

pub(crate) fn build_req_grpc_payload(req: impl Request + Serialize) -> Payload {
    tracing::debug!(
        "build_req_grpc_payload {} request_id={}",
        req.get_type_url(),
        req.get_request_id()
    );
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
    tracing::debug!(
        "build_resp_grpc_payload {} request_id={}",
        resp.get_type_url(),
        resp.get_request_id().or(Some(&"".to_string())).unwrap()
    );
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
    let body_str = String::from_utf8(body_data).unwrap();
    tracing::debug!("build_server_response {} with {}", type_url, body_str);
    if TYPE_SERVER_CHECK_SERVER_RESPONSE.eq(&type_url) {
        let de: ServerCheckServerResponse = serde_json::from_str(body_str.as_str())?;
        return Ok(Box::new(de));
    }
    if TYPE_ERROR_SERVER_RESPONSE.eq(&type_url) {
        let de: ErrorResponse = serde_json::from_str(body_str.as_str())?;
        return Ok(Box::new(de));
    }
    if TYPE_HEALTH_CHECK_SERVER_RESPONSE.eq(&type_url) {
        let de: HealthCheckServerResponse = serde_json::from_str(body_str.as_str())?;
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
    let headers = metadata.headers;
    let body_str = String::from_utf8(body_data).unwrap();
    tracing::debug!("build_server_request {} with {}", type_url, body_str);
    if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&type_url) {
        let de = ConnectResetServerRequest::from(body_str.as_str()).headers(headers);
        return Ok(Box::new(de));
    }
    if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&type_url) {
        let de = ClientDetectionServerRequest::from(body_str.as_str()).headers(headers);
        return Ok(Box::new(de));
    }
    Err(crate::api::error::Error::Deserialization(type_url))
}

/// Covert payload to PayloadInner {type_url, headers, body_str}
pub(crate) fn covert_payload(payload: Payload) -> PayloadInner {
    let metadata = payload.metadata.unwrap();
    let body_data = payload.body.unwrap().value;
    let type_url = metadata.r#type;
    let headers = metadata.headers;
    let body_str = String::from_utf8(body_data).unwrap();
    tracing::debug!("covert_payload {} with {}", type_url, body_str);
    PayloadInner {
        type_url,
        headers,
        body_str,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use crate::common::remote::request::client_request::ServerCheckClientRequest;
    use crate::common::remote::request::Request;
    use crate::common::remote::response::server_response::ServerCheckServerResponse;
    use crate::common::remote::response::Response;
    use crate::common::util::payload_helper;

    #[test]
    fn it_works_serde_json() {
        let data = r#"
        {
            "connectionId": "uuid",
            "requestId": "666",
            "resultCode": 200,
            "errorCode": 0
        }"#;
        let resp: ServerCheckServerResponse = serde_json::from_str(data).unwrap();
        println!("serde_json resp {:?}", resp);
        assert_eq!(resp.get_request_id().unwrap().as_str(), "666");
    }

    #[test]
    fn test_covert_payload1() {
        let data = r#"
        {
            "connectionId": "uuid",
            "requestId": "666",
            "resultCode": 200,
            "errorCode": 0
        }"#;
        let resp: ServerCheckServerResponse = serde_json::from_str(data).unwrap();
        let resp_type_url = resp.get_type_url().to_string();

        let payload = payload_helper::build_resp_grpc_payload(resp);
        let payload_inner = payload_helper::covert_payload(payload);

        println!("test_covert_payload1, type_url {}", &payload_inner.type_url);
        assert_eq!(resp_type_url, payload_inner.type_url);
    }

    #[test]
    fn test_covert_payload2() {
        let data = "{\"requestId\":\"666\",\"headers\":{}}";
        let req: ServerCheckClientRequest = serde_json::from_str(data).unwrap();
        let req_type_url = req.get_type_url().to_string();

        let payload = payload_helper::build_req_grpc_payload(req);
        let payload_inner = payload_helper::covert_payload(payload);

        println!("test_covert_payload2, type_url {}", &payload_inner.type_url);
        assert_eq!(req_type_url, payload_inner.type_url);
        assert_eq!(data, payload_inner.body_str.as_str());
        assert_eq!(HashMap::new(), payload_inner.headers);
    }
}
