use crate::api::plugin::RequestResource;
use nacos_macro::request;

#[request(identity = "QueryPromptRequest", module = "ai")]
pub(crate) struct QueryPromptRequest {
    pub(crate) namespace_id: Option<String>,
    pub(crate) prompt_key: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) md5: Option<String>,
}

impl QueryPromptRequest {
    pub(crate) fn ai_request_resource(&self) -> Option<RequestResource<'_>> {
        crate::ai::message::ai_resource(self.namespace_id.as_deref(), self.prompt_key.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::remote::grpc::message::{GrpcMessageData, GrpcRequestMessage};

    fn sample() -> QueryPromptRequest {
        QueryPromptRequest {
            namespace_id: Some("public".to_string()),
            prompt_key: Some("plan-prompt".to_string()),
            label: Some("latest".to_string()),
            // `requestId` is compared separately, so it has to be a string here
            request_id: Some("1".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_query_prompt_request_wire_format() {
        let mut json = serde_json::to_value(sample()).expect("request should serialize");
        let request_id = json
            .get("requestId")
            .and_then(|value| value.as_str())
            .expect("requestId should be a string");
        assert_eq!(request_id, "1");
        json.as_object_mut()
            .expect("request should serialize to an object")
            .remove("requestId");

        let expected: serde_json::Value = serde_json::from_str(
            r#"{
                "namespaceId": "public",
                "promptKey": "plan-prompt",
                "version": null,
                "label": "latest",
                "md5": null,
                "headers": {}
            }"#,
        )
        .expect("expected JSON should parse");
        assert_eq!(json, expected);
    }

    #[test]
    fn test_query_prompt_request_identity_module_and_resource() {
        assert_eq!(QueryPromptRequest::identity(), "QueryPromptRequest");
        assert_eq!(sample().module(), "ai");

        let request = sample();
        let resource = request.request_resource().expect("ai resource");
        assert_eq!(resource.request_type, "AI");
        assert_eq!(resource.namespace, Some("public"));
        assert_eq!(resource.group, Some("DEFAULT_GROUP"));
        assert_eq!(resource.resource, Some("plan-prompt"));
    }
}
