/// ConfigFilter
#[async_trait::async_trait]
pub trait ConfigFilter: Send + Sync {
    /// Filter the config_req or config_resp. You can modify their values as needed.
    ///
    /// [`ConfigReq`] and [`ConfigResp`] will not be [`None`] at the same time.
    /// Only one of [`ConfigReq`] and [`ConfigResp`] is [`Some`].
    async fn filter(
        &self,
        config_req: Option<&mut ConfigReq>,
        config_resp: Option<&mut ConfigResp>,
    );
}

/// ConfigReq for [`ConfigFilter`]
pub struct ConfigReq {
    /// DataId
    pub data_id: String,
    /// Group
    pub group: String,
    /// Namespace/Tenant
    pub namespace: String,
    /// Content
    pub content: String,
    /// Content's Encrypted Data Key.
    pub encrypted_data_key: String,
}

impl ConfigReq {
    pub fn new(
        data_id: String,
        group: String,
        namespace: String,
        content: String,
        encrypted_data_key: String,
    ) -> Self {
        ConfigReq {
            data_id,
            group,
            namespace,
            content,
            encrypted_data_key,
        }
    }
}

/// ConfigResp for [`ConfigFilter`]
pub struct ConfigResp {
    /// DataId
    pub data_id: String,
    /// Group
    pub group: String,
    /// Namespace/Tenant
    pub namespace: String,
    /// Content
    pub content: String,
    /// Content's Encrypted Data Key.
    pub encrypted_data_key: String,
}

impl ConfigResp {
    pub fn new(
        data_id: String,
        group: String,
        namespace: String,
        content: String,
        encrypted_data_key: String,
    ) -> Self {
        ConfigResp {
            data_id,
            group,
            namespace,
            content,
            encrypted_data_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::plugin::config_filter::{ConfigReq, ConfigResp};
    use crate::api::plugin::ConfigFilter;

    struct TestConfigEncryptionFilter;

    impl TestConfigEncryptionFilter {
        fn encrypt(&self, secret_key: &String, content: &String) -> String {
            secret_key.to_owned() + content
        }

        fn decrypt(&self, secret_key: &String, content: &String) -> String {
            content.replace(secret_key, "")
        }
    }

    #[async_trait::async_trait]
    impl ConfigFilter for TestConfigEncryptionFilter {
        async fn filter(
            &self,
            config_req: Option<&mut ConfigReq>,
            config_resp: Option<&mut ConfigResp>,
        ) {
            if let Some(config_req) = config_req {
                if !config_req.encrypted_data_key.is_empty() {
                    config_req.content =
                        self.encrypt(&config_req.encrypted_data_key, &config_req.content);
                }
            }

            if let Some(config_resp) = config_resp {
                if !config_resp.encrypted_data_key.is_empty() {
                    config_resp.content =
                        self.decrypt(&config_resp.encrypted_data_key, &config_resp.content);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_config_filter() {
        let config_filter = TestConfigEncryptionFilter {};

        let (data_id, group, namespace, content, encrypted_data_key) = (
            "D".to_string(),
            "G".to_string(),
            "N".to_string(),
            "C".to_string(),
            "E".to_string(),
        );

        let mut config_req = ConfigReq::new(
            data_id.clone(),
            group.clone(),
            namespace.clone(),
            content.clone(),
            encrypted_data_key.clone(),
        );
        config_filter.filter(Some(&mut config_req), None).await;

        assert_eq!(config_req.content, encrypted_data_key + content.as_str());

        let mut config_resp = ConfigResp::new(
            config_req.data_id.clone(),
            config_req.group.clone(),
            config_req.namespace.clone(),
            config_req.content.clone(),
            config_req.encrypted_data_key.clone(),
        );
        config_filter.filter(None, Some(&mut config_resp)).await;

        assert_eq!(config_resp.content, content);
    }
}
