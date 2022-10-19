use nacos_macro::request;

#[request(identity = "SubscribeServiceRequest", module = "naming")]
pub(crate) struct SubscribeServiceRequest {
    pub subscribe: bool,

    pub clusters: String,
}
