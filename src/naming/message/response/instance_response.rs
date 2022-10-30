use nacos_macro::response;

#[response(identity = "InstanceResponse", module = "naming")]
pub(crate) struct InstanceResponse {
    #[serde(rename = "type")]
    pub(crate) r_type: Option<String>,
}
