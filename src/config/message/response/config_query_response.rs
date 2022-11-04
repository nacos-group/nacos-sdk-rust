use nacos_macro::response;

const CONFIG_NOT_FOUND: i32 = 300;
const CONFIG_QUERY_CONFLICT: i32 = 400;

/// ConfigQueryResponse by server.
#[response(identity = "ConfigQueryResponse", module = "config")]
pub(crate) struct ConfigQueryResponse {
    /// json, properties, txt, html, xml, ...
    pub(crate) content_type: Option<String>,
    pub(crate) content: Option<String>,
    pub(crate) md5: Option<String>,
    /// whether content was encrypted with encryptedDataKey.
    pub(crate) encrypted_data_key: Option<String>,

    /// now is useless.
    pub(crate) tag: Option<String>,
    pub(crate) last_modified: i64,
    pub(crate) beta: bool,
}

impl ConfigQueryResponse {
    pub(crate) fn is_not_found(&self) -> bool {
        self.error_code == CONFIG_NOT_FOUND
    }
    pub(crate) fn is_query_conflict(&self) -> bool {
        self.error_code == CONFIG_QUERY_CONFLICT
    }
}
