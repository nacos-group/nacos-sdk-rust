use arc_swap::ArcSwap;
use rand::RngExt;
use std::ops::{Add, Deref};
use std::sync::Arc;
use tokio::time::{Duration, Instant};

use crate::api::plugin::{AuthContext, AuthPlugin, LoginIdentityContext};

use super::RequestResource;

pub const USERNAME: &str = "username";

pub const PASSWORD: &str = "password";

pub(crate) const ACCESS_TOKEN: &str = "accessToken";

#[allow(dead_code)]
pub(crate) const TOKEN_TTL: &str = "tokenTtl";

/// Http login AuthPlugin.
pub struct HttpLoginAuthPlugin {
    login_identity: ArcSwap<LoginIdentityContext>,
    next_login_refresh: ArcSwap<Instant>,
}

impl Default for HttpLoginAuthPlugin {
    fn default() -> Self {
        Self {
            login_identity: ArcSwap::from_pointee(LoginIdentityContext::default()),
            next_login_refresh: ArcSwap::from_pointee(Instant::now()),
        }
    }
}

#[async_trait::async_trait]
impl AuthPlugin for HttpLoginAuthPlugin {
    async fn login(&self, server_list: Arc<Vec<String>>, auth_context: Arc<AuthContext>) {
        let now_instant = Instant::now();
        if now_instant.le(self.next_login_refresh.load().deref()) {
            tracing::debug!("Http login return because now_instant lte next_login_refresh.");
            return;
        }

        let username = auth_context
            .params
            .get(USERNAME)
            .expect("Username parameter should exist for HTTP auth")
            .to_owned();
        let password = auth_context
            .params
            .get(PASSWORD)
            .expect("Password parameter should exist for HTTP auth")
            .to_owned();

        let server_addr = {
            // random one
            server_list
                .get(rand::rng().random_range(0..server_list.len()))
                .expect("Server list should not be empty")
                .to_string()
        };

        let scheme = if cfg!(feature = "tls") {
            "https"
        } else {
            "http"
        };
        let login_url = format!("{scheme}://{server_addr}/nacos/v1/auth/login");

        tracing::debug!("Http login with username={username},password={password}");

        let login_response = {
            let resp = crate::common::remote::http_client()
                .post(login_url)
                .query(&[(USERNAME, username), (PASSWORD, password)])
                .send()
                .await;
            tracing::debug!("Http login resp={resp:?}");

            match resp {
                Err(e) => {
                    tracing::error!("Http login error, send response failed, err={e:?}");
                    None
                }
                Ok(resp) => {
                    let resp_text = resp
                        .text()
                        .await
                        .expect("Response text conversion should succeed");
                    let resp_obj = serde_json::from_str::<HttpLoginResponse>(&resp_text);
                    match resp_obj {
                        Err(e) => {
                            tracing::error!("Http login error, resp_text={resp_text}, err={e:?}");
                            None
                        }
                        Ok(resp_obj) => Some(resp_obj),
                    }
                }
            }
        };

        if let Some(login_response) = login_response {
            let delay_sec = login_response.token_ttl / 10;
            let new_login_identity = Arc::new(
                LoginIdentityContext::default()
                    .add_context(ACCESS_TOKEN, login_response.access_token),
            );
            self.login_identity.store(new_login_identity);

            self.next_login_refresh
                .store(Arc::new(Instant::now().add(Duration::from_secs(delay_sec))));
        }
    }

    fn get_login_identity(&self, _: RequestResource) -> Arc<LoginIdentityContext> {
        self.login_identity.load_full()
    }
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpLoginResponse {
    access_token: String,
    token_ttl: u64,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::api::plugin::{
        ACCESS_TOKEN, AuthPlugin, HttpLoginAuthPlugin, LoginIdentityContext, RequestResource,
    };

    #[test]
    fn test_get_login_identity_shares_stored_context() {
        let http_auth_plugin = HttpLoginAuthPlugin::default();
        let stored = Arc::new(LoginIdentityContext::default().add_context(ACCESS_TOKEN, "token"));
        http_auth_plugin.login_identity.store(stored.clone());

        let first = http_auth_plugin.get_login_identity(RequestResource::default());
        let second = http_auth_plugin.get_login_identity(RequestResource::default());

        assert!(Arc::ptr_eq(&first, &stored));
        assert!(Arc::ptr_eq(&first, &second));
    }
}
