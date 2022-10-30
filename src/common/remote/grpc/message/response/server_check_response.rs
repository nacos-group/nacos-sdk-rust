use nacos_macro::response;

#[response(identity = "ServerCheckResponse", module = "internal")]
pub(crate) struct ServerCheckResponse {
    pub(crate) connection_id: Option<String>,
}
