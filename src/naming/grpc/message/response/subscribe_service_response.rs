use nacos_macro::response;

use crate::naming::dto::ServiceInfo;

#[response(identity = "SubscribeServiceResponse", module = "naming")]
pub(crate) struct SubscribeServiceResponse {
    pub service_info: ServiceInfo,
}
