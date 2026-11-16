use nacos_macro::response;

use crate::ai::model::mcp::McpServerDetailInfo;

#[response(identity = "QueryMcpServerResponse", module = "ai")]
pub(crate) struct QueryMcpServerResponse {
    pub(crate) mcp_server_detail_info: Option<McpServerDetailInfo>,
}
