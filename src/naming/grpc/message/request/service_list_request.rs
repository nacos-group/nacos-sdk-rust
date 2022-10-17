use nacos_macro::request;

#[request(identity = "ServiceListRequest", module = "naming")]
pub(crate) struct ServiceListRequest {
    pub page_no: i32,

    pub page_size: i32,

    pub selector: Option<String>,
}
