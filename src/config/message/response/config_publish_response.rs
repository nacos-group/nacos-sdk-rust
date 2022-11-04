use nacos_macro::response;

/// ConfigPublishResponse by server.
#[response(identity = "ConfigPublishResponse", module = "config")]
pub(crate) struct ConfigPublishResponse {}

impl ConfigPublishResponse {
    pub(crate) fn is_result_success(&self) -> bool {
        200 == self.result_code
    }
}
