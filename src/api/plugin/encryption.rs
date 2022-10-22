use crate::api::plugin::{ConfigFilter, ConfigReq, ConfigResp};

/**
 * For example：cipher-AES-dataId.
 */
pub const DEFAULT_CIPHER_PREFIX: &str = "cipher-";

pub const DEFAULT_CIPHER_SPLIT: &str = "-";

/// EncryptionPlugin for Config.
pub trait EncryptionPlugin: Send + Sync {
    /**
     * Whether need to do cipher.
     *
     * e.g. data_id = "cipher-AES-dataId"
     */
    fn need_cipher(&self, data_id: &str) -> bool {
        data_id.starts_with(DEFAULT_CIPHER_PREFIX)
            && self
                .parse_algorithm_name(data_id)
                .eq(&self.algorithm_name())
    }

    /**
     * Parse encryption algorithm name.
     *
     * @param data_id data_id
     * @return algorithm name
     */
    fn parse_algorithm_name(&self, data_id: &str) -> String {
        data_id
            .split(DEFAULT_CIPHER_SPLIT)
            .nth(1)
            .unwrap()
            .to_string()
    }

    /**
     * Encrypt interface.
     *
     * @param secret_key secret key
     * @param content   content unencrypted
     * @return encrypt value
     */
    fn encrypt(&self, secret_key: &str, content: &str) -> String;

    /**
     * Decrypt interface.
     *
     * @param secret_key secret key
     * @param content   encrypted
     * @return decrypt value
     */
    fn decrypt(&self, secret_key: &str, content: &str) -> String;

    /**
     * Generate secret key. It only be known by you.
     *
     * @return Secret key
     */
    fn generate_secret_key(&self) -> String;

    /**
     * Algorithm name. e.g. AES,AES128,AES256,DES,3DES,...
     *
     * @return name
     */
    fn algorithm_name(&self) -> String;

    /**
     * Encrypt secret Key. It will be transmitted in the network.
     *
     * @param secretKey secretKey
     * @return encrypted secretKey
     */
    fn encrypt_secret_key(&self, secret_key: &str) -> String;

    /**
     * Decrypt secret Key.
     *
     * @param secret_key secretKey
     * @return decrypted secretKey
     */
    fn decrypt_secret_key(&self, secret_key: &str) -> String;
}

/// ConfigEncryptionFilter handle with [`EncryptionPlugin`]
pub struct ConfigEncryptionFilter {
    encryption_plugins: Vec<Box<dyn EncryptionPlugin>>,
}

impl ConfigEncryptionFilter {
    pub fn new(encryption_plugins: Vec<Box<dyn EncryptionPlugin>>) -> Self {
        Self { encryption_plugins }
    }
}

impl ConfigFilter for ConfigEncryptionFilter {
    fn filter(&self, config_req: Option<&mut ConfigReq>, config_resp: Option<&mut ConfigResp>) {
        // Publish configuration, encrypt
        if let Some(config_req) = config_req {
            for plugin in &self.encryption_plugins {
                if !plugin.need_cipher(&config_req.data_id) {
                    continue;
                }

                let secret_key = plugin.generate_secret_key();
                let encrypted_content = plugin.encrypt(&secret_key, &config_req.content);
                let encrypted_secret_key = plugin.encrypt_secret_key(&secret_key);

                // set encrypted data.
                config_req.encrypted_data_key = encrypted_secret_key;
                config_req.content = encrypted_content;
                break;
            }
        }

        // Get configuration, decrypt
        if let Some(config_resp) = config_resp {
            if !config_resp.encrypted_data_key.is_empty() {
                for plugin in &self.encryption_plugins {
                    if !plugin.need_cipher(&config_resp.data_id) {
                        continue;
                    }

                    // get encrypted data.
                    let encrypted_secret_key = &config_resp.encrypted_data_key;
                    let encrypted_content = &config_resp.content;

                    let decrypted_secret_key = plugin.decrypt_secret_key(encrypted_secret_key);
                    let decrypted_content =
                        plugin.decrypt(&decrypted_secret_key, encrypted_content);

                    // set decrypted data.
                    config_resp.content = decrypted_content;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::plugin::config_filter::{ConfigReq, ConfigResp};
    use crate::api::plugin::encryption::DEFAULT_CIPHER_PREFIX;
    use crate::api::plugin::{ConfigEncryptionFilter, ConfigFilter, EncryptionPlugin};

    struct TestEncryptionPlugin;

    impl EncryptionPlugin for TestEncryptionPlugin {
        fn encrypt(&self, secret_key: &str, content: &str) -> String {
            secret_key.to_owned() + content
        }

        fn decrypt(&self, secret_key: &str, content: &str) -> String {
            content.replace(secret_key, "")
        }

        fn generate_secret_key(&self) -> String {
            "secret-key".to_string()
        }

        fn algorithm_name(&self) -> String {
            "TEST".to_string()
        }

        fn encrypt_secret_key(&self, secret_key: &str) -> String {
            "crypt_".to_owned() + secret_key
        }

        fn decrypt_secret_key(&self, secret_key: &str) -> String {
            secret_key.replace("crypt_", "")
        }
    }

    #[test]
    fn test_config_encryption_filters_empty() {
        let config_encryption_filter = ConfigEncryptionFilter::new(vec![]);

        let (data_id, group, namespace, content, encrypted_data_key) = (
            "D".to_string(),
            "G".to_string(),
            "N".to_string(),
            "C".to_string(),
            "".to_string(),
        );

        let mut config_req = ConfigReq::new(
            data_id.clone(),
            group.clone(),
            namespace.clone(),
            content.clone(),
            encrypted_data_key.clone(),
        );
        config_encryption_filter.filter(Some(&mut config_req), None);

        assert_eq!(config_req.content, encrypted_data_key + content.as_str());

        let mut config_resp = ConfigResp::new(
            config_req.data_id.clone(),
            config_req.group.clone(),
            config_req.namespace.clone(),
            config_req.content.clone(),
            config_req.encrypted_data_key.clone(),
        );
        config_encryption_filter.filter(None, Some(&mut config_resp));

        assert_eq!(config_resp.content, content);
    }

    #[test]
    fn test_config_encryption_filters() {
        let config_encryption_filter =
            ConfigEncryptionFilter::new(vec![Box::new(TestEncryptionPlugin {})]);

        let (data_id, group, namespace, content, encrypted_data_key) = (
            DEFAULT_CIPHER_PREFIX.to_owned() + "-TEST-D",
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
        config_encryption_filter.filter(Some(&mut config_req), None);

        let mut config_resp = ConfigResp::new(
            config_req.data_id.clone(),
            config_req.group.clone(),
            config_req.namespace.clone(),
            config_req.content.clone(),
            config_req.encrypted_data_key.clone(),
        );
        config_encryption_filter.filter(None, Some(&mut config_resp));

        assert_eq!(config_resp.content, content);
    }

    #[test]
    fn test_config_encryption_filters_not_need_cipher() {
        let config_encryption_filter =
            ConfigEncryptionFilter::new(vec![Box::new(TestEncryptionPlugin {})]);

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
        config_encryption_filter.filter(Some(&mut config_req), None);

        let mut config_resp = ConfigResp::new(
            config_req.data_id.clone(),
            config_req.group.clone(),
            config_req.namespace.clone(),
            config_req.content.clone(),
            config_req.encrypted_data_key.clone(),
        );
        config_encryption_filter.filter(None, Some(&mut config_resp));

        assert_eq!(config_resp.content, content);
    }
}
