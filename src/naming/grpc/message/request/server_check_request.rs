use nacos_macro::request;

#[request(identity = "ServerCheckRequest", module = "internal")]
pub(crate) struct ServerCheckRequest {}
