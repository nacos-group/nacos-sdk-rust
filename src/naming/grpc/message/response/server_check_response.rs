use nacos_macro::response;

#[response(identity = "serverCheckResponse")]
pub struct ServerCheckResponse {
    pub connection_id: String,
}
