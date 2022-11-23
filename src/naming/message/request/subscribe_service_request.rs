use nacos_macro::request;

use crate::common::remote::generate_request_id;

#[request(identity = "SubscribeServiceRequest", module = "naming")]
pub(crate) struct SubscribeServiceRequest {
    pub(crate) subscribe: bool,

    pub(crate) clusters: String,
}

impl SubscribeServiceRequest {
    pub(crate) fn new(
        subscribe: bool,
        clusters: String,
        service_name: Option<String>,
        namespace: Option<String>,
        group_name: Option<String>,
    ) -> Self {
        let request_id = Some(generate_request_id());
        Self {
            subscribe,
            clusters,
            request_id,
            namespace,
            service_name,
            group_name,
            ..Default::default()
        }
    }
}
