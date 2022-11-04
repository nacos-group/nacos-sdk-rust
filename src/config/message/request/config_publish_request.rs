use crate::common::remote::generate_request_id;
use nacos_macro::request;
use std::collections::HashMap;

/// ConfigPublishRequest from client.
#[request(identity = "ConfigPublishRequest", module = "config")]
pub(crate) struct ConfigPublishRequest {
    /// content
    pub(crate) content: String,
    /// Cas md5 (prev content's md5)
    pub(crate) cas_md5: Option<String>,
    /// Addition Map
    pub(crate) addition_map: HashMap<String, String>,
}

impl ConfigPublishRequest {
    pub fn new(data_id: String, group: String, namespace: String, content: String) -> Self {
        Self {
            request_id: Some(generate_request_id()),
            data_id: Some(data_id),
            group: Some(group),
            namespace: Some(namespace),
            content,
            cas_md5: None,
            addition_map: HashMap::default(),
            ..Default::default()
        }
    }
    /// Sets the cas_md5.
    pub fn cas_md5(mut self, cas_md5: Option<String>) -> Self {
        self.cas_md5 = cas_md5;
        self
    }

    /// Add into additionMap.
    pub fn add_addition_param(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.addition_map.insert(key.into(), val.into());
    }

    /// Add into additionMap.
    pub fn add_addition_params(&mut self, addition_params: HashMap<String, String>) {
        self.addition_map.extend(addition_params.into_iter());
    }
}
