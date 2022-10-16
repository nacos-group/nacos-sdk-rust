use nacos_macro::response;

#[response(identity = "InstanceResponse", module = "naming")]
pub struct InstanceResponse {
    #[serde(rename = "type")]
    pub r_type: Option<String>,
}
