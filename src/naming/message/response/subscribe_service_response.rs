use nacos_macro::response;

use crate::api::naming::ServiceInfo;

#[response(identity = "SubscribeServiceResponse", module = "naming")]
pub(crate) struct SubscribeServiceResponse {
    pub(crate) service_info: ServiceInfo,
}
