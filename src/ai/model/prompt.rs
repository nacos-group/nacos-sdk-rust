use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub prompt_key: String,
    pub version: String,
    pub template: String,
    pub md5: String,
    #[serde(default)]
    pub variables: Vec<PromptVariable>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptVariable {
    pub name: String,
    pub default_value: Option<String>,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_deserializes_captured_server_json() {
        let raw = r#"{
            "promptKey": "plan-prompt",
            "version": "1.0.0",
            "template": "Hello {{name}}",
            "md5": "68d236230e844839284da6e2d967e36f",
            "variables": [
                {"name": "name", "defaultValue": "world", "description": "who"}
            ]
        }"#;
        let prompt: Prompt = serde_json::from_str(raw).expect("prompt should parse");
        assert_eq!(prompt.prompt_key, "plan-prompt");
        assert_eq!(prompt.version, "1.0.0");
        assert_eq!(prompt.variables[0].name, "name");
        assert_eq!(prompt.variables[0].default_value.as_deref(), Some("world"));

        let round = serde_json::to_value(&prompt).expect("prompt should serialize");
        assert_eq!(round["variables"][0]["defaultValue"], "world");
    }
}
