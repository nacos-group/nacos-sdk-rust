/// Configures settings for Client.
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    pub(crate) server_addr: Option<String>,
}

impl ClientConfig {
    /// Creates a new `ClientConfig`.
    pub fn new() -> Self {
        ClientConfig { server_addr: None }
    }

    /// Sets the server addr against.
    pub fn server_addr(self, server_addr: impl Into<String>) -> Self {
        ClientConfig {
            server_addr: Some(server_addr.into()),
            ..self
        }
    }
}
