use nacos_macro::response;

#[response(identity = "ServerCheckResponse", module = "internal")]
pub(crate) struct ServerCheckResponse {
    pub(crate) connection_id: Option<String>,

    /// Present since Nacos 2.3.0. Missing means an older server.
    #[serde(default)]
    pub(crate) support_ability_negotiation: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_negotiation_field_defaults_to_false() {
        let response: ServerCheckResponse = serde_json::from_str(
            r#"{"resultCode":200,"errorCode":0,"message":null,"requestId":null,"connectionId":"c-1"}"#,
        )
        .expect("deserialize");
        assert_eq!(response.connection_id.as_deref(), Some("c-1"));
        assert!(!response.support_ability_negotiation);
    }

    #[test]
    fn test_negotiation_field_is_parsed() {
        let response: ServerCheckResponse = serde_json::from_str(
            r#"{"resultCode":200,"errorCode":0,"message":null,"requestId":null,"connectionId":"c-1","supportAbilityNegotiation":true}"#,
        )
        .expect("deserialize");
        assert!(response.support_ability_negotiation);
    }
}
