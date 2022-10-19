use nacos_macro::response;

use crate::naming::odt::ServiceInfo;

#[response(identity = "QueryServiceResponse", module = "naming")]
pub struct QueryServiceResponse {
    pub service_info: ServiceInfo,
}
