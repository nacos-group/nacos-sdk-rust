use nacos_macro::response;

use crate::api::naming::ServiceInfo;

#[response(identity = "QueryServiceResponse", module = "naming")]
pub(crate) struct QueryServiceResponse {
    pub(crate) service_info: ServiceInfo,
}
