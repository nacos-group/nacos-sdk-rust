use nacos_macro::response;

#[response(identity = "serverCheckResponse", module = "naming")]
pub struct ServerCheckResponse {
    pub connection_id: String,
}
