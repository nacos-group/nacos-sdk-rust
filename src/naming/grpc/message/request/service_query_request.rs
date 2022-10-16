use nacos_macro::request;

#[request(identity = "ServiceQueryRequest", module = "naming")]
pub(crate) struct ServiceQueryRequest {
    pub cluster: String,

    pub healthy_only: bool,

    pub udp_port: i32,
}
