use crate::api::plugin::{AuthContext, AuthPlugin, LoginIdentityContext};

pub const USERNAME: &str = "username";

pub const PASSWORD: &str = "password";

pub(crate) const ACCESS_TOKEN: &str = "accessToken";

pub(crate) const TOKEN_TTL: &str = "tokenTtl";

/// todo, please no use.
#[derive(Default)]
pub struct HttpLoginAuthPlugin {
    server_list: Vec<String>,
}

impl AuthPlugin for HttpLoginAuthPlugin {
    fn set_server_list(&mut self, server_list: Vec<String>) {
        self.server_list = server_list;
    }

    fn login(&mut self, auth_context: AuthContext) -> LoginIdentityContext {
        todo!()
    }
}
