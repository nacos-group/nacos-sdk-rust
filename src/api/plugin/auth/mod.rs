#[cfg(feature = "auth-by-http")]
mod auth_by_http;
#[cfg(feature = "auth-by-http")]
pub use auth_by_http::*;

#[cfg(feature = "auth-by-aliyun")]
mod auth_by_aliyun_ram;
#[cfg(feature = "auth-by-aliyun")]
pub use auth_by_aliyun_ram::*;

use std::{collections::HashMap, sync::Arc, time::Duration};
use tracing::{Instrument, debug, debug_span, info};

use crate::common::executor;

/// Auth plugin in Client.
/// This api may change in the future, please forgive me if you customize the implementation.
#[async_trait::async_trait]
pub trait AuthPlugin: Send + Sync {
    /// Login with [`AuthContext`], Note that this method will be scheduled continuously.
    async fn login(&self, server_list: Vec<String>, auth_context: AuthContext);

    /// Get the [`LoginIdentityContext`].
    fn get_login_identity(&self, resource: RequestResource) -> LoginIdentityContext;
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
        self.params.extend(map);
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
        self.contexts.extend(map);
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

    fn get_login_identity(&self, _: RequestResource) -> LoginIdentityContext {
        // noop
        self.login_identity.clone()
    }
}

pub async fn init_auth_plugin(
    auth_plugin: Arc<dyn AuthPlugin>,
    server_list: Vec<String>,
    auth_params: HashMap<String, String>,
    id: String,
) {
    info!("init auth task");
    let auth_context = AuthContext::default().add_params(auth_params);
    // First login
    auth_plugin
        .login(server_list.clone(), auth_context.clone())
        .in_current_span()
        .await;
    info!("init auth finish");

    executor::spawn(
        async move {
            // Periodic refresh
            info!("auth plugin task start.");
            loop {
                auth_plugin
                    .login(server_list.clone(), auth_context.clone())
                    .in_current_span()
                    .await;
                debug!("auth_plugin schedule at fixed delay");
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }
        .instrument(debug_span!("auth_task", id = id)),
    );
}

#[derive(Debug, Default)]
pub struct RequestResource {
    pub request_type: String,
    pub namespace: Option<String>,
    pub group: Option<String>,
    pub resource: Option<String>,
}
