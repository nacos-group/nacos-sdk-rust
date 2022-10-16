use nacos_macro::request;

#[request(identity = "ServerCheckRequest", module = "naming")]
pub(crate) struct ServerCheckRequest {}
