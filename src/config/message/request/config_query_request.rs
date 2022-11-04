use crate::common::remote::generate_request_id;
use nacos_macro::request;

/// ConfigQueryRequest from client.
#[request(identity = "ConfigQueryRequest", module = "config")]
pub(crate) struct ConfigQueryRequest {}

impl ConfigQueryRequest {
    pub fn new(data_id: String, group: String, namespace: String) -> Self {
        Self {
            request_id: Some(generate_request_id()),
            data_id: Some(data_id),
            group: Some(group),
            namespace: Some(namespace),
            ..Default::default()
        }
    }
}
