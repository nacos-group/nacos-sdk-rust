use nacos_macro::response;

/// ConfigRemoveResponse by server.
#[response(identity = "ConfigRemoveResponse", module = "config")]
pub(crate) struct ConfigRemoveResponse {}
