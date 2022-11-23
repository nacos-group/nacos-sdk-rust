use nacos_macro::request;

use crate::common::remote::generate_request_id;

#[request(identity = "ServerCheckRequest", module = "internal")]
pub(crate) struct ServerCheckRequest {}

impl ServerCheckRequest {
    pub(crate) fn new() -> Self {
        let request_id = Some(generate_request_id());

        Self {
            request_id,
            ..Default::default()
        }
    }
}
