use nacos_macro::response;

#[response(identity = "ServiceListResponse", module = "naming")]
pub(crate) struct ServiceListResponse {
    pub(crate) count: i32,

    pub(crate) service_names: Vec<String>,
}
