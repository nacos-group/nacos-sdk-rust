use nacos_macro::request;

/// ConfigChangeNotifyRequest from server.
#[request(identity = "ConfigChangeNotifyRequest", module = "config")]
pub(crate) struct ConfigChangeNotifyRequest {}
