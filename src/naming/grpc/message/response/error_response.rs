use nacos_macro::response;

#[response(identity = "ErrorResponse", module = "naming")]
pub struct ErrorResponse {}
