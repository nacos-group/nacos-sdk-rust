use nacos_macro::response;

use crate::naming::dto::ServiceInfo;

#[response(identity = "QueryServiceResponse", module = "naming")]
pub struct QueryServiceResponse {
    pub service_info: ServiceInfo,
}
