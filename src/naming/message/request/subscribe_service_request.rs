use nacos_macro::request;

#[request(identity = "SubscribeServiceRequest", module = "naming")]
pub(crate) struct SubscribeServiceRequest {
    pub(crate) subscribe: bool,

    pub(crate) clusters: String,
}
