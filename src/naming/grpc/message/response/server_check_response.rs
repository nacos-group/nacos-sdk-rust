use nacos_macro::response;

#[response(identity = "ServerCheckResponse", module = "internal")]
pub struct ServerCheckResponse {
    pub connection_id: Option<String>,
}
