use prost_types::Any;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use tracing::warn;

use crate::api::error::Error::ErrResponse;
use crate::api::error::Error::ErrResult;
use crate::api::error::Error::Serialization;
use crate::api::error::Result;
use crate::api::plugin::RequestResource;
use crate::common::remote::grpc::message::response::ErrorResponse;
use crate::nacos_proto::v2::{Metadata, Payload};
use std::fmt::Debug;
use tracing::error;

/// Helper macro to convert payload body to target type with consistent error handling.
/// Logs the payload content if conversion fails for easier debugging.
macro_rules! try_convert_payload {
    ($result:expr, $body_any:expr, $target_type:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => {
                match std::str::from_utf8(&$body_any.value) {
                    Ok(payload_str) => {
                        error!(
                            "payload {} can not convert to {} occur an error:{:?}",
                            payload_str, $target_type, error
                        );
                    }
                    Err(_) => {
                        error!(
                            "can not convert to target type {}, this payload can not convert to string as well",
                            $target_type
                        );
                    }
                }
                return Err(error);
            }
        }
    };
}

pub(crate) mod request;
pub(crate) mod response;

#[derive(Debug)]
pub(crate) struct GrpcMessage<T>
where
    T: GrpcMessageData,
{
    headers: HashMap<String, String>,
    body: T,
    client_ip: String,
}

impl<T> GrpcMessage<T>
where
    T: GrpcMessageData,
{
    #[allow(dead_code)]
    pub(crate) fn body(&self) -> &T {
        &self.body
    }

    pub(crate) fn into_body(self) -> T {
        self.body
    }

    pub(crate) fn into_payload(self) -> Result<Payload> {
        let mut payload = Payload::default();
        let meta_data = Metadata {
            r#type: T::identity().to_string(),
            client_ip: self.client_ip.to_string(),
            headers: self.headers,
        };

        let body = match self.body.to_proto_any() {
            Ok(proto_any) => proto_any,
            Err(error) => {
                error!("Serialize GrpcMessageBody occur an error: {:?}", error);
                return Err(error);
            }
        };

        payload.metadata = Some(meta_data);
        payload.body = Some(body);
        Ok(payload)
    }

    pub(crate) fn from_payload(payload: Payload) -> Result<Self> {
        let body_any = match payload.body {
            Some(body) => body,
            None => return Err(ErrResult("grpc payload body is empty".to_string())),
        };

        let meta_data = payload.metadata.unwrap_or_default();
        let r_type = meta_data.r#type;
        let client_ip = meta_data.client_ip;
        let headers = meta_data.headers;

        // try to serialize target type if r_type is not empty
        if !r_type.is_empty() {
            if T::identity().eq(&r_type) {
                let ret: Result<T> = T::from_proto_any(&body_any);
                let de_body = try_convert_payload!(ret, body_any, T::identity());
                return Ok(GrpcMessage {
                    headers,
                    body: de_body,
                    client_ip,
                });
            }

            // type mismatch - try to convert to ErrorResponse
            warn!(
                "payload type {}, target type {}, trying convert to ErrorResponse",
                &r_type,
                T::identity()
            );
            let ret: Result<ErrorResponse> = ErrorResponse::from_proto_any(&body_any);
            let error_response = try_convert_payload!(ret, body_any, "ErrorResponse");
            return Err(ErrResponse(
                error_response.request_id,
                error_response.result_code,
                error_response.error_code,
                error_response.message,
            ));
        }

        // empty r_type - try direct conversion
        warn!("payload type is empty!");
        let ret: Result<T> = T::from_proto_any(&body_any);
        if let Ok(de_body) = ret {
            return Ok(GrpcMessage {
                headers,
                body: de_body,
                client_ip,
            });
        }

        // direct conversion failed - log warning and try ErrorResponse
        let error = ret.unwrap_err();
        if let Ok(payload_str) = std::str::from_utf8(&body_any.value) {
            warn!(
                "payload {} can not convert to {} occur an error:{:?}",
                payload_str,
                T::identity(),
                error
            );
        }

        let ret: Result<ErrorResponse> = ErrorResponse::from_proto_any(&body_any);
        if let Ok(error_response) = ret {
            return Err(ErrResponse(
                error_response.request_id,
                error_response.result_code,
                error_response.error_code,
                error_response.message,
            ));
        }

        // both conversions failed - return original error
        error!(
            "trying convert to ErrorResponse occur an error:{:?}",
            ret.unwrap_err()
        );
        Err(error)
    }
}

pub(crate) trait GrpcMessageData:
    Debug + Clone + Serialize + DeserializeOwned + Send
{
    fn identity<'a>() -> std::borrow::Cow<'a, str>;

    fn to_proto_any(&self) -> Result<Any> {
        let mut any = Any {
            type_url: Self::identity().to_string(),
            ..Default::default()
        };
        let byte_data = match serde_json::to_vec(self) {
            Ok(data) => data,
            Err(error) => {
                return Err(Serialization(error));
            }
        };
        any.value = byte_data;
        Ok(any)
    }

    fn from_proto_any<T: GrpcMessageData>(any: &Any) -> Result<T> {
        let body = match serde_json::from_slice(&any.value) {
            Ok(data) => data,
            Err(error) => {
                return Err(Serialization(error));
            }
        };
        Ok(body)
    }
}

#[allow(dead_code)]
pub(crate) trait GrpcRequestMessage: GrpcMessageData {
    fn header(&self, key: &str) -> Option<&String>;

    fn headers(&self) -> &HashMap<String, String>;

    fn take_headers(&mut self) -> HashMap<String, String>;

    fn add_headers(&mut self, map: HashMap<String, String>);

    fn request_id(&self) -> Option<&String>;

    fn module(&self) -> &str;

    fn request_resource(&self) -> Option<RequestResource>;
}

#[allow(dead_code)]
pub(crate) trait GrpcResponseMessage: GrpcMessageData {
    fn request_id(&self) -> Option<&String>;

    fn result_code(&self) -> i32;

    fn error_code(&self) -> i32;

    fn message(&self) -> Option<&String>;

    fn is_success(&self) -> bool;
}

pub(crate) struct GrpcMessageBuilder<T>
where
    T: GrpcMessageData,
{
    headers: HashMap<String, String>,
    body: T,
    client_ip: String,
}

impl<T> GrpcMessageBuilder<T>
where
    T: GrpcMessageData,
{
    pub(crate) fn new(body: T) -> Self {
        GrpcMessageBuilder {
            headers: HashMap::<String, String>::new(),
            body,
            client_ip: crate::common::util::LOCAL_IP.to_owned(),
        }
    }

    pub(crate) fn header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    pub(crate) fn headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers.extend(headers);
        self
    }

    pub(crate) fn build(self) -> GrpcMessage<T> {
        GrpcMessage {
            headers: self.headers,
            body: self.body,
            client_ip: self.client_ip,
        }
    }
}
