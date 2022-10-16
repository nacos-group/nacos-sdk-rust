use nacos_macro::response;

#[response(identity = "BatchInstanceResponse", module = "naming")]
pub struct BatchInstanceResponse {
    #[serde(rename = "type")]
    pub r_type: Option<String>,
}
