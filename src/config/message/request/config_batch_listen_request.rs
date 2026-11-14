use crate::common::remote::generate_request_id;
use nacos_macro::request;
use serde::{Deserialize, Serialize};

/// ConfigBatchListenRequest from client.
#[request(identity = "ConfigBatchListenRequest", module = "config")]
pub(crate) struct ConfigBatchListenRequest {
    /// listen or remove-listen.
    pub(crate) listen: bool,
    /// context of listen.
    pub(crate) config_listen_contexts: Vec<ConfigListenContext>,
}

impl ConfigBatchListenRequest {
    pub fn new(listen: bool) -> Self {
        Self {
            listen,
            config_listen_contexts: Vec::new(),
            request_id: Some(generate_request_id()),
            ..Default::default()
        }
    }

    /// Set ConfigListenContext.
    pub fn config_listen_context(mut self, contexts: Vec<ConfigListenContext>) -> Self {
        self.config_listen_contexts = contexts;
        self
    }

    /// Add ConfigListenContext.
    pub fn add_config_listen_context(mut self, context: ConfigListenContext) -> Self {
        self.config_listen_contexts.push(context);
        self
    }
}

/// The Context of config listen.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigListenContext {
    /// DataId
    #[serde(rename = "dataId")]
    data_id: String,
    /// Group
    group: String,
    /// tenant
    tenant: String,
    /// Md5
    md5: String,
}

impl ConfigListenContext {
    pub fn new(data_id: String, group: String, tenant: String, md5: String) -> Self {
        ConfigListenContext {
            data_id,
            group,
            tenant,
            md5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_listen_context_wire_format() {
        let context = ConfigListenContext::new(
            "data_id".to_string(),
            "group".to_string(),
            "namespace".to_string(),
            "md5".to_string(),
        );
        let json = serde_json::to_string(&context).expect("context should serialize");
        assert_eq!(
            json,
            r#"{"dataId":"data_id","group":"group","tenant":"namespace","md5":"md5"}"#
        );

        let deserialized: ConfigListenContext =
            serde_json::from_str(&json).expect("context should deserialize");
        assert_eq!(deserialized.data_id, "data_id");
        assert_eq!(deserialized.group, "group");
        assert_eq!(deserialized.tenant, "namespace");
        assert_eq!(deserialized.md5, "md5");
    }
}
