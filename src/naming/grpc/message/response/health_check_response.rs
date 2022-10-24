use nacos_macro::response;

#[response(identity = "HealthCheckResponse", module = "internal")]
pub struct HealthCheckResponse {}
