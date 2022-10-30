use nacos_macro::request;

#[request(identity = "ServiceQueryRequest", module = "naming")]
pub(crate) struct ServiceQueryRequest {
    pub(crate) cluster: String,

    pub(crate) healthy_only: bool,

    pub(crate) udp_port: i32,
}
