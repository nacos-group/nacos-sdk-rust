use crate::ai::model::mcp::{
    McpEndpointSpec, McpResourceSpecification, McpServerBasicInfo, McpToolSpecification,
};
use crate::api::plugin::RequestResource;
use nacos_macro::request;

#[request(identity = "ReleaseMcpServerRequest", module = "ai")]
pub(crate) struct ReleaseMcpServerRequest {
    pub(crate) namespace_id: Option<String>,
    pub(crate) mcp_id: Option<String>,
    pub(crate) mcp_name: Option<String>,
    pub(crate) server_specification: Option<McpServerBasicInfo>,
    pub(crate) tool_specification: Option<McpToolSpecification>,
    pub(crate) resource_specification: Option<McpResourceSpecification>,
    pub(crate) endpoint_specification: Option<McpEndpointSpec>,
    pub(crate) create_draft: bool,
}

impl ReleaseMcpServerRequest {
    pub(crate) fn ai_request_resource(&self) -> Option<RequestResource> {
        crate::ai::message::ai_resource(self.namespace_id.clone(), self.mcp_name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::remote::grpc::message::{GrpcMessageData, GrpcRequestMessage};

    fn sample() -> ReleaseMcpServerRequest {
        let basic: McpServerBasicInfo = serde_json::from_str(
            r#"{"name": "plan-mcp", "protocol": "stdio", "versionDetail": {"version": "1.0.0", "is_latest": true}}"#,
        )
        .expect("basic should parse");
        ReleaseMcpServerRequest {
            namespace_id: Some("public".to_string()),
            mcp_name: Some("plan-mcp".to_string()),
            server_specification: Some(basic),
            create_draft: true,
            ..Default::default()
        }
    }

    #[test]
    fn test_release_mcp_server_request_wire_format() {
        let mut json = serde_json::to_value(sample()).expect("request should serialize");
        json.as_object_mut()
            .expect("request should serialize to an object")
            .remove("requestId");
        assert_eq!(json["namespaceId"], "public");
        assert_eq!(json["mcpName"], "plan-mcp");
        assert_eq!(json["createDraft"], true);
        assert_eq!(json["serverSpecification"]["name"], "plan-mcp");
        assert!(json.get("toolSpecification").is_some());
        assert!(json.get("resourceSpecification").is_some());
        assert!(json.get("endpointSpecification").is_some());
        assert!(json.get("headers").is_some());
    }

    #[test]
    fn test_release_mcp_server_request_resource() {
        assert_eq!(
            ReleaseMcpServerRequest::identity(),
            "ReleaseMcpServerRequest"
        );
        assert_eq!(sample().module(), "ai");
        assert_eq!(
            sample()
                .request_resource()
                .expect("ai resource")
                .resource
                .as_deref(),
            Some("plan-mcp")
        );
    }
}
