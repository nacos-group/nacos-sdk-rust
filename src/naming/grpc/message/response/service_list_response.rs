use nacos_macro::response;

#[response(identity = "ServiceListResponse", module = "naming")]
pub(crate) struct ServiceListResponse {
    pub count: i32,

    pub service_names: Vec<String>,
}
