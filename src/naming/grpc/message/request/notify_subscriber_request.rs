use nacos_macro::request;

use crate::naming::odt::ServiceInfo;

#[request(identity = "NotifySubscriberRequest", module = "naming")]
pub(crate) struct NotifySubscriberRequest {
    pub service_info: ServiceInfo,
}
