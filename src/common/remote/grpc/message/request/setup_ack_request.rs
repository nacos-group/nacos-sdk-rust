use std::collections::HashMap;

use nacos_macro::request;

#[request(identity = "SetupAckRequest", module = "internal")]
pub(crate) struct SetupAckRequest {
    pub(crate) ability_table: Option<HashMap<String, bool>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_ack_parses_ability_table() {
        let ack: SetupAckRequest = serde_json::from_str(
            r#"{"headers":{},"requestId":null,"abilityTable":{"supportPersistentInstanceByGrpc":true}}"#,
        )
        .expect("setup ack should deserialize");
        let table = ack.ability_table.expect("ability table");
        assert_eq!(table.get("supportPersistentInstanceByGrpc"), Some(&true));
    }

    #[test]
    fn test_setup_ack_allows_null_ability_table() {
        let ack: SetupAckRequest =
            serde_json::from_str(r#"{"headers":{},"requestId":null,"abilityTable":null}"#)
                .expect("null ability table should deserialize");
        assert!(ack.ability_table.is_none());
    }
}
