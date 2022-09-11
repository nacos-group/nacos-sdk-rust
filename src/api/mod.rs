pub mod client_config;
pub mod constants;
pub mod error;

#[cfg(feature = "config")]
pub trait ConfigService {
    /// Get config, return the content.
    fn get_config(&self, data_id: String, group: String, timeout_ms: u32) -> error::Result<String>;

    /// Listen the config change.
    fn listen(
        &mut self,
        data_id: String,
        group: String,
    ) -> error::Result<std::sync::mpsc::Receiver<ConfigResponse>>;
}

#[cfg(feature = "config")]
pub struct ConfigResponse {
    /// Namespace/Tenant
    namespace: String,
    /// DataId
    data_id: String,
    /// Group
    group: String,
    /// Content
    content: String,
}

#[cfg(feature = "config")]
impl ConfigResponse {
    pub fn get_namespace(&self) -> &String {
        &self.namespace
    }
    pub fn get_data_id(&self) -> &String {
        &self.data_id
    }
    pub fn get_group(&self) -> &String {
        &self.group
    }
    pub fn get_content(&self) -> &String {
        &self.content
    }
}
