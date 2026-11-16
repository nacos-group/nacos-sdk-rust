use nacos_macro::response;

#[response(identity = "McpServerEndpointResponse", module = "ai")]
pub(crate) struct McpServerEndpointResponse {
    #[serde(rename = "type")]
    pub(crate) r#type: Option<String>,
}
