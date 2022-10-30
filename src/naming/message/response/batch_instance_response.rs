use nacos_macro::response;

#[response(identity = "BatchInstanceResponse", module = "naming")]
pub(crate) struct BatchInstanceResponse {
    #[serde(rename = "type")]
    pub(crate) r_type: Option<String>,
}
