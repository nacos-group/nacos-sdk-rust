use nacos_macro::request;

#[request(identity = "HealthCheckRequest", module = "internal")]
pub(crate) struct HealthCheckRequest {}
