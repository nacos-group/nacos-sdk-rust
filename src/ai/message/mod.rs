pub(crate) mod request;
pub(crate) mod response;

use crate::api::error::Error;
use crate::api::plugin::RequestResource;

pub(crate) const NOT_FOUND_CODE: i32 = 404;
pub(crate) const INVALID_PARAM_CODE: i32 = 400;

pub(crate) fn is_not_found(error: &Error) -> bool {
    matches!(error, Error::ErrResponse(_, _, NOT_FOUND_CODE, _))
}

pub(crate) fn is_permanent_rejection(error: &Error) -> bool {
    matches!(
        error,
        Error::ErrResponse(_, _, NOT_FOUND_CODE | INVALID_PARAM_CODE, _)
    )
}

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

#[cfg(test)]
mod tests {
    use super::{is_not_found, is_permanent_rejection};
    use crate::api::error::Error;

    fn err_response(code: i32) -> Error {
        Error::ErrResponse(None, 200, code, None)
    }

    #[test]
    fn test_error_classification_by_response_code() {
        assert!(is_not_found(&err_response(404)));
        assert!(!is_not_found(&err_response(400)));
        assert!(!is_not_found(&err_response(500)));

        assert!(is_permanent_rejection(&err_response(404)));
        assert!(is_permanent_rejection(&err_response(400)));
        assert!(!is_permanent_rejection(&err_response(500)));
        assert!(!is_permanent_rejection(&err_response(409)));
    }
}
