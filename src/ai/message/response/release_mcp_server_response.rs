use nacos_macro::response;

#[response(identity = "ReleaseMcpServerResponse", module = "ai")]
pub(crate) struct ReleaseMcpServerResponse {
    pub(crate) mcp_id: Option<String>,
}
