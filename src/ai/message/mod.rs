pub(crate) mod request;
pub(crate) mod response;

use crate::api::error::Error;
use crate::api::plugin::RequestResource;
use crate::common::remote::grpc::message::GrpcResponseMessage;

pub(crate) const NOT_FOUND_CODE: i32 = 404;
pub(crate) const INVALID_PARAM_CODE: i32 = 400;

pub(crate) fn ai_resource(
    namespace_id: Option<String>,
    resource: Option<String>,
) -> Option<RequestResource> {
    Some(RequestResource {
        request_type: "AI".to_string(),
        namespace: namespace_id,
        group: Some(crate::api::constants::DEFAULT_GROUP.to_string()),
        resource,
    })
}

pub(crate) fn response_error<T: GrpcResponseMessage>(response: &T) -> Error {
    Error::ErrResponse(
        None,
        response.result_code(),
        response.error_code(),
        response.message().cloned(),
    )
}
