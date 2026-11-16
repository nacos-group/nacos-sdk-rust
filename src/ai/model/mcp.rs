use std::collections::HashMap;
use std::ops::Deref;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct McpServerBasicInfo {
    pub namespace_id: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub protocol: Option<String>,
    pub front_protocol: Option<String>,
    pub description: Option<String>,
    pub repository: Option<Repository>,
    pub packages: Option<Vec<Value>>,
    pub icons: Option<Vec<Value>>,
    pub website_url: Option<String>,
    pub version_detail: Option<ServerVersionDetail>,
    pub version: Option<String>,
    pub remote_server_config: Option<McpServerRemoteServiceConfig>,
    pub local_server_config: Option<Value>,
    pub enabled: bool,
    pub status: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

impl Default for McpServerBasicInfo {
    fn default() -> Self {
        Self {
            namespace_id: None,
            id: None,
            name: None,
            protocol: None,
            front_protocol: None,
            description: None,
            repository: None,
            packages: None,
            icons: None,
            website_url: None,
            version_detail: None,
            version: None,
            remote_server_config: None,
            local_server_config: None,
            enabled: true,
            status: Some("active".to_string()),
            capabilities: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDetailInfo {
    #[serde(flatten)]
    pub basic: McpServerBasicInfo,
    pub backend_endpoints: Option<Vec<McpEndpointInfo>>,
    pub frontend_endpoints: Option<Vec<McpEndpointInfo>>,
    pub tool_spec: Option<McpToolSpecification>,
    pub resource_spec: Option<McpResourceSpecification>,
    pub all_versions: Option<Vec<ServerVersionDetail>>,
}

impl Deref for McpServerDetailInfo {
    type Target = McpServerBasicInfo;

    fn deref(&self) -> &Self::Target {
        &self.basic
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ServerVersionDetail {
    pub version: String,
    pub release_date: Option<String>,
    pub is_latest: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEndpointInfo {
    pub protocol: Option<String>,
    pub address: String,
    pub port: i32,
    pub path: Option<String>,
    pub headers: Option<Vec<Value>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSpecification {
    pub specification_type: Option<String>,
    pub encrypt_data: Option<Value>,
    #[serde(default)]
    pub tools: Vec<McpTool>,
    #[serde(default)]
    pub tools_meta: HashMap<String, McpToolMeta>,
    #[serde(default)]
    pub security_schemes: Vec<Value>,
    #[serde(default)]
    pub extensions: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    #[serde(rename = "_meta")]
    pub meta: Option<Value>,
    pub annotations: Option<McpToolAnnotations>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct McpToolMeta {
    pub invoke_context: Option<HashMap<String, String>>,
    pub enabled: bool,
    pub templates: Option<Value>,
}

impl Default for McpToolMeta {
    fn default() -> Self {
        Self {
            invoke_context: None,
            enabled: true,
            templates: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAnnotations {
    pub title: Option<String>,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceSpecification {
    pub specification_type: Option<String>,
    pub encrypt_data: Option<Value>,
    #[serde(default)]
    pub resources: Vec<Value>,
    #[serde(default)]
    pub resource_templates: Vec<Value>,
    #[serde(default)]
    pub extensions: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEndpointSpec {
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    #[serde(default)]
    pub data: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRemoteServiceConfig {
    pub service_ref: Option<Value>,
    pub export_path: Option<String>,
    pub front_endpoint_config_list: Option<Vec<Value>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub url: Option<String>,
    pub source: Option<String>,
    pub id: Option<String>,
    pub subfolder: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURED_DETAIL: &str = r#"{
        "allVersions": [{"is_latest": true, "release_date": "2026-11-16T04:30:50Z", "version": "1.0.0"}],
        "backendEndpoints": null,
        "capabilities": ["TOOL", "RESOURCE"],
        "description": "design capture",
        "enabled": true,
        "frontProtocol": null,
        "frontendEndpoints": null,
        "icons": null,
        "id": "cd780a12-d51b-4552-9d03-05ef4547934d",
        "localServerConfig": {"command": "echo", "args": ["openapi-it"]},
        "name": "plan-mcp",
        "namespaceId": "public",
        "packages": null,
        "protocol": "stdio",
        "remoteServerConfig": null,
        "repository": null,
        "resourceSpec": {
            "encryptData": null,
            "extensions": {},
            "resourceTemplates": [],
            "resources": [{"name": "note", "uri": "file:///tmp/note.txt", "description": "note", "mimeType": "text/plain"}],
            "specificationType": null
        },
        "status": "active",
        "toolSpec": {
            "encryptData": null,
            "extensions": {},
            "securitySchemes": [],
            "specificationType": null,
            "tools": [{
                "_meta": null,
                "annotations": null,
                "description": "Echo text",
                "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}},
                "name": "echo",
                "outputSchema": null
            }],
            "toolsMeta": {}
        },
        "version": "1.0.0",
        "versionDetail": {"is_latest": true, "release_date": "2026-11-16T04:30:50Z", "version": "1.0.0"},
        "websiteUrl": null
    }"#;

    #[test]
    fn test_detail_deserializes_captured_server_json() {
        let detail: McpServerDetailInfo =
            serde_json::from_str(CAPTURED_DETAIL).expect("detail should parse");
        assert_eq!(detail.name.as_deref(), Some("plan-mcp"));
        assert_eq!(detail.protocol.as_deref(), Some("stdio"));
        assert_eq!(
            detail.capabilities.as_ref().expect("capabilities"),
            &["TOOL", "RESOURCE"]
        );
        assert_eq!(detail.status.as_deref(), Some("active"));
        let version = detail.version_detail.as_ref().expect("versionDetail");
        assert_eq!(version.is_latest, Some(true));
        assert_eq!(version.version, "1.0.0");
        assert_eq!(
            version.release_date.as_deref(),
            Some("2026-11-16T04:30:50Z")
        );
        let tool = &detail.tool_spec.as_ref().expect("toolSpec").tools[0];
        assert_eq!(tool.name, "echo");
        // The captured server response carries `"_meta": null`.
        assert!(tool.meta.is_none());
        assert_eq!(detail.all_versions.as_ref().expect("allVersions").len(), 1);
    }

    #[test]
    fn test_detail_round_trip_keeps_wire_names() {
        let detail: McpServerDetailInfo =
            serde_json::from_str(CAPTURED_DETAIL).expect("detail should parse");
        let value = serde_json::to_value(&detail).expect("detail should serialize");
        assert!(value.get("toolSpec").is_some());
        assert!(value["toolSpec"]["tools"][0].get("_meta").is_some());
        assert_eq!(value["versionDetail"]["is_latest"], true);
        assert_eq!(
            value["versionDetail"]["release_date"],
            "2026-11-16T04:30:50Z"
        );
        assert_eq!(value["name"], "plan-mcp");
    }

    #[test]
    fn test_version_detail_defaults_missing_latest_flag() {
        let detail: McpServerDetailInfo =
            serde_json::from_str(r#"{"name": "plan-mcp", "versionDetail": {"version": "1.0.0"}}"#)
                .expect("detail without is_latest should parse");
        let version = detail.version_detail.as_ref().expect("versionDetail");
        assert_eq!(version.is_latest, None);
    }

    #[test]
    fn test_defaults_match_java_enabled_and_status() {
        let basic = McpServerBasicInfo::default();
        assert!(basic.enabled);
        assert_eq!(basic.status.as_deref(), Some("active"));
        assert!(McpToolMeta::default().enabled);
    }

    #[test]
    fn test_missing_enabled_deserializes_true() {
        let basic: McpServerBasicInfo =
            serde_json::from_str(r#"{"name": "svc"}"#).expect("basic should parse");
        assert!(basic.enabled);
        assert_eq!(basic.status.as_deref(), Some("active"));
        let meta: McpToolMeta = serde_json::from_str("{}").expect("meta should parse");
        assert!(meta.enabled);
    }

    #[test]
    fn test_endpoint_spec_type_field_keeps_wire_name() {
        let spec: McpEndpointSpec =
            serde_json::from_str(r#"{"type": "http", "data": {"path": "/mcp"}}"#)
                .expect("endpoint spec should parse");
        assert_eq!(spec.r#type.as_deref(), Some("http"));
        assert_eq!(spec.data["path"], "/mcp");
        let value = serde_json::to_value(&spec).expect("endpoint spec should serialize");
        assert_eq!(value["type"], "http");
    }
}
