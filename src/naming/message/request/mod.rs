mod batch_instance_request;
mod instance_request;
mod notify_subscriber_request;
mod persistent_instance_request;
mod service_list_request;
mod service_query_request;
mod subscribe_service_request;

pub(crate) use batch_instance_request::*;
pub(crate) use instance_request::*;
pub(crate) use notify_subscriber_request::*;
pub(crate) use persistent_instance_request::*;
pub(crate) use service_list_request::*;
pub(crate) use service_query_request::*;
pub(crate) use subscribe_service_request::*;

/// The operation carried by an instance registration request.
///
/// Shared by [`InstanceRequest`] and [`PersistentInstanceRequest`], which
/// differ only in the identity the Nacos server dispatches on.
pub(crate) enum RequestType {
    Register,
    Deregister,
}

impl RequestType {
    pub(crate) fn request_code(&self) -> String {
        match self {
            RequestType::Register => "registerInstance".to_string(),
            RequestType::Deregister => "deregisterInstance".to_string(),
        }
    }
}
