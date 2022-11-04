use nacos_macro::response;

/// ConfigChangeNotifyResponse by client.
#[response(identity = "ConfigChangeNotifyResponse", module = "config")]
pub(crate) struct ConfigChangeNotifyResponse {}

impl ConfigChangeNotifyResponse {
    /// Set request_id.
    pub(crate) fn request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }
}
