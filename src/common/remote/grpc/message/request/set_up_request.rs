use std::collections::HashMap;

use nacos_macro::request;

#[request(identity = "ConnectionSetupRequest", module = "internal")]
pub(crate) struct ConnectionSetupRequest {
    pub(crate) client_version: String,

    pub(crate) tenant: String,

    pub(crate) labels: HashMap<String, String>,

    /// Flat ability table keyed by Nacos `AbilityKey` names. An empty table is
    /// sent deliberately: the server replies with `SetupAckRequest` when the
    /// field is non-null, and this SDK implements none of the optional keys.
    pub(crate) ability_table: Option<HashMap<String, bool>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_request_sends_flat_ability_table() {
        let request = ConnectionSetupRequest {
            client_version: "Nacos-Rust-Client:0.0.0".to_string(),
            tenant: "public".to_string(),
            labels: HashMap::from([("source".to_string(), "sdk".to_string())]),
            ability_table: Some(HashMap::new()),
            ..Default::default()
        };
        let json = serde_json::to_value(&request).expect("setup request should serialize");
        assert_eq!(json["abilityTable"], serde_json::json!({}));
        assert!(
            json.get("abilities").is_none(),
            "legacy nested abilities must not be sent"
        );
        assert_eq!(json["tenant"], "public");
    }
}
