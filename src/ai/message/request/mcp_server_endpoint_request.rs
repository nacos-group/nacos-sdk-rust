use crate::api::plugin::RequestResource;
use nacos_macro::request;

#[request(identity = "McpServerEndpointRequest", module = "ai")]
pub(crate) struct McpServerEndpointRequest {
    pub(crate) namespace_id: Option<String>,
    pub(crate) mcp_id: Option<String>,
    pub(crate) mcp_name: Option<String>,
    pub(crate) address: Option<String>,
    pub(crate) port: i32,
    pub(crate) version: Option<String>,
    #[serde(rename = "type")]
    pub(crate) r#type: Option<String>,
}

impl McpServerEndpointRequest {
    pub(crate) const REGISTER_ENDPOINT: &'static str = "registerEndpoint";
    pub(crate) const DEREGISTER_ENDPOINT: &'static str = "deregisterEndpoint";

    pub(crate) fn ai_request_resource(&self) -> Option<RequestResource<'_>> {
        crate::ai::message::ai_resource(self.namespace_id.as_deref(), self.mcp_name.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::remote::grpc::message::{GrpcMessageData, GrpcRequestMessage};

    fn sample() -> McpServerEndpointRequest {
        McpServerEndpointRequest {
            namespace_id: Some("public".to_string()),
            mcp_name: Some("plan-mcp".to_string()),
            address: Some("127.0.0.1".to_string()),
            port: 18080,
            version: Some("1.0.0".to_string()),
            r#type: Some(McpServerEndpointRequest::REGISTER_ENDPOINT.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_mcp_server_endpoint_request_wire_format() {
        let mut json = serde_json::to_value(sample()).expect("request should serialize");
        json.as_object_mut()
            .expect("request should serialize to an object")
            .remove("requestId");
        let expected: serde_json::Value = serde_json::from_str(
            r#"{
                "namespaceId": "public",
                "mcpId": null,
                "mcpName": "plan-mcp",
                "address": "127.0.0.1",
                "port": 18080,
                "version": "1.0.0",
                "type": "registerEndpoint",
                "headers": {}
            }"#,
        )
        .expect("expected JSON should parse");
        assert_eq!(json, expected);
    }

    #[test]
    fn test_mcp_server_endpoint_request_identity_and_resource() {
        assert_eq!(
            McpServerEndpointRequest::identity(),
            "McpServerEndpointRequest"
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
