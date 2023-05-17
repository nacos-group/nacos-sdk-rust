#[cfg(feature = "auth-by-http")]
mod auth_by_http;
#[cfg(feature = "auth-by-http")]
pub use auth_by_http::*;

use std::collections::HashMap;

/// Auth plugin in Client.
/// This api may change in the future, please forgive me if you customize the implementation.
#[async_trait::async_trait]
pub trait AuthPlugin: Send + Sync {
    /// Login with [`AuthContext`], Note that this method will be scheduled continuously.
    async fn login(&self, server_list: Vec<String>, auth_context: AuthContext);

    /// Get the [`LoginIdentityContext`].
    fn get_login_identity(&self) -> LoginIdentityContext;
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

    /// Add the params.
    pub fn add_params(mut self, map: HashMap<String, String>) -> Self {
        self.params.extend(map.into_iter());
        self
    }
}

#[derive(Clone, Default)]
pub struct LoginIdentityContext {
    pub(crate) contexts: HashMap<String, String>,
}

impl LoginIdentityContext {
    /// Add the context.
    pub fn add_context(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.contexts.insert(key.into(), val.into());
        self
    }

    /// Add the contexts.
    pub fn add_contexts(mut self, map: HashMap<String, String>) -> Self {
        self.contexts.extend(map.into_iter());
        self
    }
}

/// Noop AuthPlugin.
#[derive(Default)]
pub(crate) struct NoopAuthPlugin {
    login_identity: LoginIdentityContext,
}

#[async_trait::async_trait]
impl AuthPlugin for NoopAuthPlugin {
    #[allow(unused_variables)]
    async fn login(&self, server_list: Vec<String>, auth_context: AuthContext) {
        // noop
    }

    fn get_login_identity(&self) -> LoginIdentityContext {
        // noop
        self.login_identity.clone()
    }
}
