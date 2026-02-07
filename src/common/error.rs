use crate::api::error::Error;
use crate::api::error::Result;
use crate::common::remote::grpc::message::GrpcResponseMessage;

/// Handles a standard gRPC response and converts it to a Result.
/// Returns Ok(()) if the response is successful, otherwise returns an Err with the error details.
pub(crate) fn handle_response<T: GrpcResponseMessage>(response: &T, operation: &str) -> Result<()> {
    if response.is_success() {
        Ok(())
    } else {
        Err(Error::ErrResult(to_err_msg(response, operation)))
    }
}

pub(crate) fn to_err_msg<T: GrpcResponseMessage>(response: &T, operation: &str) -> String {
    format!(
        "handle {} failed: result_code={}, error_code={}, message={}",
        operation,
        response.result_code(),
        response.error_code(),
        response.message().map(|s| s.as_str()).unwrap_or("")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::remote::grpc::message::GrpcMessageData;
    use serde::{Deserialize, Serialize};
    use std::borrow::Cow;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct MockResponse {
        result_code: i32,
        error_code: i32,
        message: Option<String>,
    }

    impl GrpcMessageData for MockResponse {
        fn identity<'a>() -> Cow<'a, str> {
            Cow::Borrowed("MockResponse")
        }
    }

    impl GrpcResponseMessage for MockResponse {
        fn request_id(&self) -> Option<&String> {
            None
        }

        fn result_code(&self) -> i32 {
            self.result_code
        }

        fn error_code(&self) -> i32 {
            self.error_code
        }

        fn message(&self) -> Option<&String> {
            self.message.as_ref()
        }

        fn is_success(&self) -> bool {
            self.result_code == 200
        }
    }

    #[test]
    fn test_handle_response_success() {
        let response = MockResponse {
            result_code: 200,
            error_code: 0,
            message: None,
        };
        assert!(handle_response(&response, "test_handle_response_success").is_ok());
    }

    #[test]
    fn test_handle_response_failure() {
        let response = MockResponse {
            result_code: 500,
            error_code: 400,
            message: Some("test error".to_string()),
        };
        let result = handle_response(&response, "test_handle_response_failure");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("error_code=400"));
        assert!(err_msg.contains("test error"));
    }
}
