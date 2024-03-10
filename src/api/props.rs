use std::collections::HashMap;

use crate::api::constants::*;
use crate::properties::{get_value, get_value_bool, get_value_option, get_value_u32};

/// Configures settings for Client.
#[derive(Debug, Clone)]
pub struct ClientProps {
    /// server_addr e.g: 127.0.0.1:8848; 192.168.0.1
    server_addr: String,
    /// grpc port
    grpc_port: Option<u32>,
    /// public is "", Should define a more meaningful namespace
    namespace: String,
    /// app_name
    app_name: String,
    /// naming push_empty_protection, default true
    naming_push_empty_protection: bool,
    /// naming load_cache_at_start, default false
    naming_load_cache_at_start: bool,
    /// env_first when get props, default true
    env_first: bool,
    /// metadata
    labels: HashMap<String, String>,
    /// client_version
    client_version: String,
    /// auth context
    auth_context: HashMap<String, String>,
}

impl ClientProps {
    pub(crate) fn get_server_addr(&self) -> String {
        if self.env_first {
            get_value(
                ENV_NACOS_CLIENT_COMMON_SERVER_ADDRESS,
                self.server_addr.clone(),
            )
        } else {
            self.server_addr.clone()
        }
    }

    pub(crate) fn get_remote_grpc_port(&self) -> Option<u32> {
        self.grpc_port
    }

    pub(crate) fn get_namespace(&self) -> String {
        if self.env_first {
            get_value(ENV_NACOS_CLIENT_COMMON_NAMESPACE, self.namespace.clone())
        } else {
            self.namespace.clone()
        }
    }

    pub(crate) fn get_app_name(&self) -> String {
        if self.env_first {
            get_value(ENV_NACOS_CLIENT_COMMON_APP_NAME, self.app_name.clone())
        } else {
            self.app_name.clone()
        }
    }

    pub(crate) fn get_naming_push_empty_protection(&self) -> bool {
        if self.env_first {
            get_value_bool(
                ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION,
                self.naming_push_empty_protection,
            )
        } else {
            self.naming_push_empty_protection
        }
    }

    pub(crate) fn get_naming_load_cache_at_start(&self) -> bool {
        if self.env_first {
            get_value_bool(
                ENV_NACOS_CLIENT_NAMING_LOAD_CACHE_AT_START,
                self.naming_load_cache_at_start,
            )
        } else {
            self.naming_load_cache_at_start
        }
    }

    pub(crate) fn get_labels(&self) -> HashMap<String, String> {
        let mut labels = self.labels.clone();
        labels.insert(KEY_LABEL_APP_NAME.to_string(), self.get_app_name());
        labels
    }

    pub(crate) fn get_client_version(&self) -> String {
        self.client_version.clone()
    }

    pub(crate) fn get_auth_context(&self) -> HashMap<String, String> {
        let mut auth_context = self.auth_context.clone();
        if self.env_first {
            if let Some(u) = get_value_option(ENV_NACOS_CLIENT_AUTH_USERNAME) {
                auth_context.insert(crate::api::plugin::USERNAME.into(), u);
            }
            if let Some(p) = get_value_option(ENV_NACOS_CLIENT_AUTH_PASSWORD) {
                auth_context.insert(crate::api::plugin::PASSWORD.into(), p);
            }
        }
        auth_context
    }

    pub(crate) fn get_server_list(&self) -> crate::api::error::Result<Vec<String>> {
        let server_addr = self.get_server_addr();
        if server_addr.trim().is_empty() {
            return Err(crate::api::error::Error::WrongServerAddress(String::from(
                "Server address is empty",
            )));
        }
        let hosts: Vec<&str> = server_addr.trim().split(',').collect::<Vec<&str>>();
        let mut result = vec![];
        for host in hosts {
            let host_port = host.split(':').collect::<Vec<&str>>();
            if host_port.len() == 1 {
                result.push(format!(
                    "{}:{}",
                    host,
                    get_value_u32(ENV_NACOS_CLIENT_COMMON_SERVER_PORT, DEFAULT_SERVER_PORT,)
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

        ClientProps {
            server_addr: String::from(DEFAULT_SERVER_ADDR),
            namespace: String::from(""),
            app_name: UNKNOWN.to_string(),
            naming_push_empty_protection: true,
            naming_load_cache_at_start: false,
            env_first: true,
            labels: HashMap::default(),
            client_version,
            auth_context: HashMap::default(),
            grpc_port: None,
        }
    }

    /// Sets the server addr.
    pub fn server_addr(mut self, server_addr: impl Into<String>) -> Self {
        self.server_addr = server_addr.into();
        self
    }

    /// Sets the grpc port
    pub fn remote_grpc_port(mut self, grpc_port: u32) -> Self {
        self.grpc_port = Some(grpc_port);
        self
    }

    /// Sets the namespace.
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Sets the app_name.
    pub fn app_name(mut self, app_name: impl Into<String>) -> Self {
        self.app_name = app_name.into();
        self
    }

    /// Sets the naming_push_empty_protection.
    pub fn naming_push_empty_protection(mut self, naming_push_empty_protection: bool) -> Self {
        self.naming_push_empty_protection = naming_push_empty_protection;
        self
    }

    /// Sets the naming_load_cache_at_start.
    pub fn naming_load_cache_at_start(mut self, naming_load_cache_at_start: bool) -> Self {
        self.naming_load_cache_at_start = naming_load_cache_at_start;
        self
    }

    /// Sets the env_first.
    pub fn env_first(mut self, env_first: bool) -> Self {
        self.env_first = env_first;
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
        self.auth_context
            .insert(crate::api::plugin::USERNAME.into(), username.into());
        self
    }

    /// Add auth password.
    #[cfg(feature = "auth-by-http")]
    pub fn auth_password(mut self, password: impl Into<String>) -> Self {
        self.auth_context
            .insert(crate::api::plugin::PASSWORD.into(), password.into());
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
            naming_load_cache_at_start: false,
            env_first: true,
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
