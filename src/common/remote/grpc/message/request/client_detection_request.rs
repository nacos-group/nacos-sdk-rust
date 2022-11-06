use nacos_macro::request;

#[request(identity = "ClientDetectionRequest", module = "internal")]
pub(crate) struct ClientDetectionRequest {}
