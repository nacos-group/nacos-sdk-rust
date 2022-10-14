use nacos_macro::request;

#[request(identity = "ServerCheckRequest")]
pub(crate) struct ServerCheckRequest {}
