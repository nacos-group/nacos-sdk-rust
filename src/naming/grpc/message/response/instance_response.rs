use nacos_macro::response;

#[response(identity = "InstanceResponse")]
pub struct InstanceResponse {
    pub r_type: Option<String>,
}
