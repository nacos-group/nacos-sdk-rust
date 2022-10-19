use nacos_macro::response;

use crate::naming::odt::ServiceInfo;

#[response(identity = "SubscribeServiceResponse", module = "naming")]
pub(crate) struct SubscribeServiceResponse {
    pub service_info: ServiceInfo,
}
