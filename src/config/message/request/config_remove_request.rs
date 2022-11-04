use crate::common::remote::generate_request_id;
use nacos_macro::request;

/// ConfigRemoveRequest from client.
#[request(identity = "ConfigRemoveRequest", module = "config")]
pub(crate) struct ConfigRemoveRequest {
    /// tag
    pub(crate) tag: Option<String>,
}

impl ConfigRemoveRequest {
    pub fn new(data_id: String, group: String, namespace: String) -> Self {
        Self {
            request_id: Some(generate_request_id()),
            data_id: Some(data_id),
            group: Some(group),
            namespace: Some(namespace),
            tag: None,
            ..Default::default()
        }
    }
}
