use nacos_macro::response;

/// ConfigPublishResponse by server.
#[response(identity = "ConfigPublishResponse", module = "config")]
pub(crate) struct ConfigPublishResponse {}
