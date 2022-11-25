use nacos_macro::request;

use crate::api::naming::ServiceInfo;

#[request(identity = "NotifySubscriberRequest", module = "naming")]
pub(crate) struct NotifySubscriberRequest {
    pub(crate) service_info: ServiceInfo,
}
