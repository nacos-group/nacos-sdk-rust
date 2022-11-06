use nacos_macro::response;

#[response(identity = "ClientDetectionResponse", module = "internal")]
pub(crate) struct ClientDetectionResponse {}
