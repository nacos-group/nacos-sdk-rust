use nacos_macro::response;
use serde::{Deserialize, Serialize};

/// ConfigChangeBatchListenResponse by server.
#[response(identity = "ConfigChangeBatchListenResponse", module = "config")]
pub(crate) struct ConfigChangeBatchListenResponse {
    pub(crate) changed_configs: Option<Vec<ConfigContext>>,
}

/// The Context of config changed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigContext {
    /// DataId
    #[serde(rename = "dataId")]
    pub(crate) data_id: String,
    /// Group
    pub(crate) group: String,
    /// Namespace/Tenant
    #[serde(rename = "tenant")]
    pub(crate) namespace: String,
}
