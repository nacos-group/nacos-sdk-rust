use std::{error::Error, fmt::Display};

#[derive(Debug)]
pub enum ErrorKind {
    ResponseError {
        result_code: i32,

        error_code: i32,

        message: String,
    },

    SerdeJsonError(serde_json::Error),

    NoneMetaDataError,

    NonePayloadBody,

    UnknownError(String),
}
#[derive(Debug)]
pub struct NacosError {
    pub kind: ErrorKind,
}

impl Display for NacosError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ErrorKind::ResponseError {
                result_code,
                error_code,
                message,
            } => {
                write!(
                    f,
                    "response error. result_code: {:?}, error_code: {:?}, message: {:?}",
                    result_code, error_code, message
                )
            }
            ErrorKind::SerdeJsonError(error) => {
                write!(f, "serde json error. {:?}", error)
            }
            ErrorKind::UnknownError(msg) => {
                write!(f, "unknown error:{:?}", msg)
            }
            ErrorKind::NoneMetaDataError => {
                write!(f, "meta data is none")
            }
            ErrorKind::NonePayloadBody => {
                write!(f, "payload body is none")
            }
        }
    }
}

impl Error for NacosError {}
