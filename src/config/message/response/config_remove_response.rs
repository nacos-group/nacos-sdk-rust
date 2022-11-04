use nacos_macro::response;

/// ConfigRemoveResponse by server.
#[response(identity = "ConfigRemoveResponse", module = "config")]
pub(crate) struct ConfigRemoveResponse {}

impl ConfigRemoveResponse {
    pub(crate) fn is_result_success(&self) -> bool {
        200 == self.result_code
    }
}
