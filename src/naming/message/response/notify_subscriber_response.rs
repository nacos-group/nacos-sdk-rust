use nacos_macro::response;

#[response(identity = "NotifySubscriberResponse", module = "naming")]
pub(crate) struct NotifySubscriberResponse {}
