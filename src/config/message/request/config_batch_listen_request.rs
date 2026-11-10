use crate::common::remote::generate_request_id;
use nacos_macro::request;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    data_id: Arc<String>,
    /// Group
    group: Arc<String>,
    /// tenant
    tenant: Arc<String>,
    /// Md5
    md5: Arc<String>,
}

impl ConfigListenContext {
    pub fn new(
        data_id: Arc<String>,
        group: Arc<String>,
        tenant: Arc<String>,
        md5: Arc<String>,
    ) -> Self {
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
            Arc::new("data_id".to_string()),
            Arc::new("group".to_string()),
            Arc::new("namespace".to_string()),
            Arc::new("md5".to_string()),
        );
        let json = serde_json::to_string(&context).expect("context should serialize");
        assert_eq!(
            json,
            r#"{"dataId":"data_id","group":"group","tenant":"namespace","md5":"md5"}"#
        );

        let deserialized: ConfigListenContext =
            serde_json::from_str(&json).expect("context should deserialize");
        assert_eq!(*deserialized.data_id, "data_id");
        assert_eq!(*deserialized.group, "group");
        assert_eq!(*deserialized.tenant, "namespace");
        assert_eq!(*deserialized.md5, "md5");
    }

    #[test]
    fn test_config_listen_context_shares_strings() {
        let data_id = Arc::new("data_id".to_string());
        let group = Arc::new("group".to_string());
        let tenant = Arc::new("namespace".to_string());
        let md5 = Arc::new("md5".to_string());

        let context = ConfigListenContext::new(
            Arc::clone(&data_id),
            Arc::clone(&group),
            Arc::clone(&tenant),
            Arc::clone(&md5),
        );

        assert!(Arc::ptr_eq(&context.data_id, &data_id));
        assert!(Arc::ptr_eq(&context.group, &group));
        assert!(Arc::ptr_eq(&context.tenant, &tenant));
        assert!(Arc::ptr_eq(&context.md5, &md5));
    }
}
