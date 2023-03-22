use rand::Rng;
use std::ops::{Add, Deref};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::api::plugin::{AuthContext, AuthPlugin, LoginIdentityContext};

pub const USERNAME: &str = "username";

pub const PASSWORD: &str = "password";

pub(crate) const ACCESS_TOKEN: &str = "accessToken";

#[allow(dead_code)]
pub(crate) const TOKEN_TTL: &str = "tokenTtl";

/// Http login AuthPlugin.
pub struct HttpLoginAuthPlugin {
    server_list: RwLock<Vec<String>>,
    scheme: RwLock<Option<String>>,
    login_identity: RwLock<LoginIdentityContext>,
    next_login_refresh: RwLock<Instant>,
}

impl Default for HttpLoginAuthPlugin {
    fn default() -> Self {
        Self {
            server_list: RwLock::new(vec![]),
            scheme: RwLock::new(None),
            login_identity: RwLock::new(LoginIdentityContext::default()),
            next_login_refresh: RwLock::new(Instant::now()),
        }
    }
}

impl AuthPlugin for HttpLoginAuthPlugin {
    fn set_server_list(&self, server_list: Vec<String>) {
        if let Ok(mut mutex) = self.server_list.write() {
            *mutex = server_list;
        }
    }

    fn set_scheme(&self, scheme: &str) {
        if let Ok(mut mutex) = self.scheme.write() {
            *mutex = Some(scheme.to_string())
        }
    }

    fn login(&self, auth_context: AuthContext) {
        let now_instant = Instant::now();
        if now_instant.le(self.next_login_refresh.read().unwrap().deref()) {
            tracing::debug!("Http login return because now_instant lte next_login_refresh.");
            return;
        }

        let username = auth_context.params.get(USERNAME).unwrap();
        let password = auth_context.params.get(PASSWORD).unwrap();

        let server_addr = {
            let mutex = self.server_list.read().unwrap();
            // random one
            mutex
                .get(rand::thread_rng().gen_range(0..mutex.len()))
                .unwrap()
                .to_string()
        };

        let login_url = {
            let mutex = self.scheme.read().unwrap();
            let scheme = match &*mutex {
                Some(scheme) => scheme.clone(),
                None => "http".to_string(),
            };
            format!("{scheme}://{server_addr}/nacos/v1/auth/login",)
        };

        tracing::debug!(
            "Http login with username={},password={}",
            username,
            password
        );

        let future = async {
            let resp = reqwest::Client::new()
                .post(login_url)
                .query(&[(USERNAME, username), (PASSWORD, password)])
                .send()
                .await;
            tracing::debug!("Http login resp={:?}", resp);

            if resp.is_err() {
                return None;
            }

            let resp_text = resp.unwrap().text().await.unwrap();

            let resp_obj = serde_json::from_str::<HttpLoginResponse>(&resp_text);
            if resp_obj.is_err() {
                return None;
            }
            Some(resp_obj.unwrap())
        };

        let login_response = futures::executor::block_on(future);

        if let Some(login_response) = login_response {
            let delay_sec = login_response.token_ttl / 10;
            let new_login_identity = LoginIdentityContext::default()
                .add_context(ACCESS_TOKEN, login_response.access_token);

            if let Ok(mut mutex) = self.next_login_refresh.write() {
                *mutex = Instant::now().add(Duration::from_secs(delay_sec));
            }
            if let Ok(mut mutex) = self.login_identity.write() {
                *mutex = new_login_identity;
            }
        }
    }

    fn get_login_identity(&self) -> LoginIdentityContext {
        if let Ok(mutex) = self.login_identity.read() {
            mutex.to_owned()
        } else {
            LoginIdentityContext::default()
        }
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
    use crate::api::plugin::{AuthContext, AuthPlugin, HttpLoginAuthPlugin};

    #[tokio::test]
    #[ignore]
    async fn test_http_login_auth_plugin() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let http_auth_plugin = HttpLoginAuthPlugin::default();
        http_auth_plugin.set_server_list(vec!["0.0.0.0:8848".to_string()]);

        let auth_context = AuthContext::default()
            .add_param(crate::api::plugin::USERNAME, "nacos")
            .add_param(crate::api::plugin::PASSWORD, "nacos");

        http_auth_plugin.login(auth_context.clone());
        let login_identity_1 = http_auth_plugin.get_login_identity();
        assert_eq!(login_identity_1.contexts.len(), 1);

        tokio::time::sleep(tokio::time::Duration::from_millis(111)).await;

        http_auth_plugin.login(auth_context);
        let login_identity_2 = http_auth_plugin.get_login_identity();
        assert_eq!(login_identity_1.contexts, login_identity_2.contexts)
    }
}
