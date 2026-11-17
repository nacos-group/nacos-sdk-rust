pub(crate) mod request;
pub(crate) mod response;

use crate::api::error::Error;
use crate::api::plugin::RequestResource;
use crate::common::remote::grpc::message::GrpcResponseMessage;

pub(crate) const NOT_FOUND_CODE: i32 = 404;
pub(crate) const INVALID_PARAM_CODE: i32 = 400;

pub(crate) fn ai_resource<'a>(
    namespace_id: Option<&'a str>,
    resource: Option<&'a str>,
) -> Option<RequestResource<'a>> {
    Some(RequestResource {
        request_type: "AI",
        namespace: namespace_id,
        group: Some(crate::api::constants::DEFAULT_GROUP),
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
