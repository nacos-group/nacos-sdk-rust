use crate::api::plugin::RequestResource;
use nacos_macro::request;

#[request(identity = "QueryMcpServerRequest", module = "ai")]
pub(crate) struct QueryMcpServerRequest {
    pub(crate) namespace_id: Option<String>,
    pub(crate) mcp_id: Option<String>,
    pub(crate) mcp_name: Option<String>,
    pub(crate) version: Option<String>,
}

impl QueryMcpServerRequest {
    pub(crate) fn ai_request_resource(&self) -> Option<RequestResource<'_>> {
        crate::ai::message::ai_resource(self.namespace_id.as_deref(), self.mcp_name.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::remote::grpc::message::{GrpcMessageData, GrpcRequestMessage};

    fn sample() -> QueryMcpServerRequest {
        QueryMcpServerRequest {
            namespace_id: Some("public".to_string()),
            mcp_name: Some("plan-mcp".to_string()),
            version: Some("1.0.0".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_query_mcp_server_request_wire_format() {
        let mut json = serde_json::to_value(sample()).expect("request should serialize");
        json.as_object_mut()
            .expect("request should serialize to an object")
            .remove("requestId");
        let expected: serde_json::Value = serde_json::from_str(
            r#"{
                "namespaceId": "public",
                "mcpId": null,
                "mcpName": "plan-mcp",
                "version": "1.0.0",
                "headers": {}
            }"#,
        )
        .expect("expected JSON should parse");
        assert_eq!(json, expected);
    }

    #[test]
    fn test_query_mcp_server_request_resource() {
        assert_eq!(QueryMcpServerRequest::identity(), "QueryMcpServerRequest");
        assert_eq!(sample().module(), "ai");
        let request = sample();
        let resource = request.request_resource().expect("ai resource");
        assert_eq!(resource.request_type, "AI");
        assert_eq!(resource.resource.as_deref(), Some("plan-mcp"));
    }
}
