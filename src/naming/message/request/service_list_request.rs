use nacos_macro::request;

#[request(identity = "ServiceListRequest", module = "naming")]
pub(crate) struct ServiceListRequest {
    pub(crate) page_no: i32,

    pub(crate) page_size: i32,

    pub(crate) selector: Option<String>,
}
