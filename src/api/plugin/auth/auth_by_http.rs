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
    login_identity: RwLock<LoginIdentityContext>,
    next_login_refresh: RwLock<Instant>,
}

impl Default for HttpLoginAuthPlugin {
    fn default() -> Self {
        Self {
            server_list: RwLock::new(vec![]),
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

    fn login(&self, auth_context: AuthContext) {
        let now_instant = Instant::now();
        if now_instant.le(self.next_login_refresh.read().unwrap().deref()) {
            return;
        }

        let username = auth_context.params.get(USERNAME).unwrap();
        let password = auth_context.params.get(PASSWORD).unwrap();

        let mut server_addr = String::new();
        {
            let mutex = self.server_list.read().unwrap();
            // todo random one
            server_addr = mutex.first().unwrap().to_string();
        }

        // todo support https
        let login_url = format!(
            "http://{}/nacos/v1/auth/login?username={}&password={}",
            server_addr, username, password
        );

        tracing::debug!(
            "Http login with username={},password={}",
            username,
            password
        );

        let future = async {
            let resp = reqwest::Client::new().post(login_url).send().await;
            tracing::debug!("Http login resp={:?}", resp);

            if resp.is_err() {
                return HttpLoginResponse::default();
            }

            let resp_text = resp.unwrap().text().await.unwrap();
            serde_json::from_str::<HttpLoginResponse>(&resp_text).unwrap()
        };

        let login_response = futures::executor::block_on(future);

        let delay_sec = login_response.token_ttl / 10;
        let new_login_identity =
            LoginIdentityContext::default().add_context(ACCESS_TOKEN, login_response.access_token);

        if let Ok(mut mutex) = self.next_login_refresh.write() {
            *mutex = Instant::now().add(Duration::from_secs(delay_sec));
        }
        if let Ok(mut mutex) = self.login_identity.write() {
            *mutex = new_login_identity;
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
