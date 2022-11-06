#[cfg(feature = "auth-by-http")]
mod auth_by_http;
#[cfg(feature = "auth-by-http")]
pub use auth_by_http::*;

use std::collections::HashMap;

/// Auth plugin in Client.
/// This api may change in the future, please forgive me if you customize the implementation.
pub trait AuthPlugin: Send + Sync {
    /// Please hold the server_list. If the server changes, this method will be called again.
    fn set_server_list(&mut self, server_list: Vec<String>);

    /// Login with [`AuthContext`] and get the [`LoginIdentityContext`].
    fn login(&mut self, auth_context: AuthContext) -> LoginIdentityContext;
}

#[derive(Clone, Default)]
pub struct AuthContext {
    pub(crate) params: HashMap<String, String>,
}

impl AuthContext {
    /// Add the param.
    pub fn add_param(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.params.insert(key.into(), val.into());
        self
    }
}

#[derive(Clone, Default)]
pub struct LoginIdentityContext {
    pub(crate) contexts: HashMap<String, String>,
}
