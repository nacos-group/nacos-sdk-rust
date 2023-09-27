use std::collections::HashMap;

use crate::api::constants::DEFAULT_SERVER_ADDR;
use crate::properties::{get_value, get_value_bool, get_value_u32};

/// Configures settings for Client.
#[derive(Debug, Clone)]
pub struct ClientProps {
    /// server_addr e.g: 127.0.0.1:8848; 192.168.0.1
    pub(crate) server_addr: String,
    /// grpc port
    pub(crate) grpc_port: Option<u32>,
    pub(crate) namespace: String,
    /// app_name
    pub(crate) app_name: String,
    /// naming push_empty_protection, default true
    pub(crate) naming_push_empty_protection: bool,
    /// metadata
    pub(crate) labels: HashMap<String, String>,
    /// client_version
    pub(crate) client_version: String,
    /// auth context
    pub(crate) auth_context: HashMap<String, String>,
}

impl ClientProps {
    pub(crate) fn get_server_list(&self) -> crate::api::error::Result<Vec<String>> {
        if self.server_addr.trim().is_empty() {
            return Err(crate::api::error::Error::WrongServerAddress(String::from(
                "Server address is empty",
            )));
        }
        let hosts: Vec<&str> = self.server_addr.trim().split(',').collect::<Vec<&str>>();
        let mut result = vec![];
        for host in hosts {
            let host_port = host.split(':').collect::<Vec<&str>>();
            if host_port.len() == 1 {
                result.push(format!(
                    "{}:{}",
                    host,
                    get_value_u32(
                        crate::api::constants::ENV_NACOS_CLIENT_COMMON_SERVER_PORT,
                        crate::api::constants::DEFAULT_SERVER_PORT,
                    )
                ));
                continue;
            }
            result.push(host.to_string());
        }

        Ok(result)
    }
}

#[allow(clippy::new_without_default)]
impl ClientProps {
    /// Creates a new `ClientConfig`.
    pub fn new() -> Self {
        let env_project_version = env!("CARGO_PKG_VERSION");
        let client_version = format!("Nacos-Rust-Client:{}", env_project_version);

        let server_addr = get_value(
            crate::api::constants::ENV_NACOS_CLIENT_COMMON_SERVER_ADDRESS,
            DEFAULT_SERVER_ADDR,
        );

        // public is "", Should define a more meaningful namespace
        let namespace = get_value(crate::api::constants::ENV_NACOS_CLIENT_COMMON_NAMESPACE, "");

        let app_name = get_value(
            crate::api::constants::ENV_NACOS_CLIENT_COMMON_APP_NAME,
            crate::api::constants::UNKNOWN,
        );
        let mut labels = HashMap::default();
        labels.insert(
            crate::api::constants::KEY_LABEL_APP_NAME.to_string(),
            app_name.clone(),
        );

        let naming_push_empty_protection = get_value_bool(
            crate::api::constants::ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION,
            true,
        );

        ClientProps {
            server_addr,
            namespace,
            app_name,
            naming_push_empty_protection,
            labels,
            client_version,
            auth_context: HashMap::default(),
            grpc_port: None,
        }
    }

    /// Sets the server addr.
    pub fn server_addr(mut self, server_addr: impl Into<String>) -> Self {
        self.server_addr = get_value(
            crate::api::constants::ENV_NACOS_CLIENT_COMMON_SERVER_ADDRESS,
            server_addr.into(),
        );
        self
    }

    /// Sets the grpc port
    pub fn remote_grpc_port(mut self, grpc_port: u32) -> Self {
        self.grpc_port = Some(grpc_port);
        self
    }

    /// Sets the namespace.
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = get_value(
            crate::api::constants::ENV_NACOS_CLIENT_COMMON_NAMESPACE,
            namespace.into(),
        );
        self
    }

    /// Sets the app_name.
    pub fn app_name(mut self, app_name: impl Into<String>) -> Self {
        let name = get_value(
            crate::api::constants::ENV_NACOS_CLIENT_COMMON_APP_NAME,
            app_name.into(),
        );
        self.app_name = name.clone();
        self.labels
            .insert(crate::api::constants::KEY_LABEL_APP_NAME.to_string(), name);
        self
    }

    /// Sets the naming_push_empty_protection.
    pub fn naming_push_empty_protection(mut self, naming_push_empty_protection: bool) -> Self {
        self.naming_push_empty_protection = get_value_bool(
            crate::api::constants::ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION,
            naming_push_empty_protection,
        );
        self
    }

    /// Sets the labels.
    pub fn labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
        self
    }

    /// Add auth username.
    #[cfg(feature = "auth-by-http")]
    pub fn auth_username(mut self, username: impl Into<String>) -> Self {
        self.auth_context.insert(
            crate::api::plugin::USERNAME.into(),
            get_value(
                crate::api::constants::ENV_NACOS_CLIENT_AUTH_USERNAME,
                username.into(),
            ),
        );
        self
    }

    /// Add auth password.
    #[cfg(feature = "auth-by-http")]
    pub fn auth_password(mut self, password: impl Into<String>) -> Self {
        self.auth_context.insert(
            crate::api::plugin::PASSWORD.into(),
            get_value(
                crate::api::constants::ENV_NACOS_CLIENT_AUTH_PASSWORD,
                password.into(),
            ),
        );
        self
    }

    /// Add auth ext params.
    pub fn auth_ext(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.auth_context.insert(key.into(), val.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::api::error::Error;

    use super::*;

    #[test]
    fn test_get_server_list() {
        let client_props = ClientProps {
            server_addr: "127.0.0.1:8848,192.168.0.1".to_string(),
            grpc_port: Some(8888),
            namespace: "test_namespace".to_string(),
            app_name: "test_app".to_string(),
            naming_push_empty_protection: true,
            labels: HashMap::new(),
            client_version: "test_version".to_string(),
            auth_context: HashMap::new(),
        };

        let result = client_props.get_server_list();
        assert!(result.is_ok());
        match result {
            Ok(ref vec) => {
                assert!(vec.contains(&"127.0.0.1:8848".to_string()));
                assert!(vec.contains(&"192.168.0.1:8848".to_string()));
            }
            Err(_) => {}
        }
        let client_props = client_props.server_addr(String::from("     "));
        let result1 = client_props.get_server_list();
        assert!(result1.is_err());
        assert_eq!(
            format!("{}", result1.err().unwrap()),
            format!(
                "{}",
                Error::WrongServerAddress("Server address is empty".to_string())
            )
        );
    }
}
