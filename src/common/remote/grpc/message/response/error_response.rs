use nacos_macro::response;

#[response(identity = "ErrorResponse", module = "internal")]
pub(crate) struct ErrorResponse {}
