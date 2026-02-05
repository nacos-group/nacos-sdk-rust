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

        let body = self.body.to_proto_any();

        if let Err(error) = body {
            error!("Serialize GrpcMessageBody occur an error: {:?}", error);
            return Err(error);
        }
        let body = body.expect("Body should exist after checking serialize result is not error");

        payload.metadata = Some(meta_data);
        payload.body = Some(body);
        Ok(payload)
    }

    pub(crate) fn from_payload(payload: Payload) -> Result<Self> {
        let body = payload.body;
        if body.is_none() {
            return Err(ErrResult("grpc payload body is empty".to_string()));
        }

        let body_any = body.expect("Body should exist after checking it's not none");

        let meta_data = payload.metadata.unwrap_or_default();
        let r_type = meta_data.r#type;
        let client_ip = meta_data.client_ip;
        let headers = meta_data.headers;

        let de_body;

        // try to serialize target type if r_type is not empty
        if !r_type.is_empty() {
            if T::identity().eq(&r_type) {
                let ret: Result<T> = T::from_proto_any(&body_any);
                if let Err(error) = ret {
                    let payload_str = std::str::from_utf8(&body_any.value);
                    if payload_str.is_err() {
                        error!(
                            "can not convert to target type {}, this payload can not convert to string as well",
                            T::identity()
                        );
                        return Err(error);
                    }
                    let payload_str = payload_str.expect(
                        "Payload string should exist after checking utf8 conversion is not error",
                    );
                    error!(
                        "payload {} can not convert to {} occur an error:{:?}",
                        payload_str,
                        T::identity(),
                        error
                    );
                    return Err(error);
                }
                de_body = ret.expect("Deserialize body should exist after checking it's not error");
            } else {
                warn!(
                    "payload type {}, target type {}, trying convert to ErrorResponse",
                    &r_type,
                    T::identity()
                );
                // try to convert to Error Response
                let ret: Result<ErrorResponse> = ErrorResponse::from_proto_any(&body_any);
                if let Err(error) = ret {
                    let payload_str = std::str::from_utf8(&body_any.value);
                    if payload_str.is_err() {
                        error!(
                            "can not convert to ErrorResponse, this payload can not convert to string as well"
                        );
                        return Err(error);
                    }
                    let payload_str = payload_str.expect(
                        "Payload string should exist after checking utf8 conversion is not error",
                    );
                    error!(
                        "payload {} can not convert to ErrorResponse occur an error:{:?}",
                        payload_str, error
                    );
                    return Err(error);
                }

                let error_response = ret.expect(
                    "Error response should exist after checking deserialize result is not error",
                );
                return Err(ErrResponse(
                    error_response.request_id,
                    error_response.result_code,
                    error_response.error_code,
                    error_response.message,
                ));
            }
        } else {
            warn!("payload type is empty!");
            let ret: Result<T> = T::from_proto_any(&body_any);
            if let Err(error) = ret {
                let payload_str = std::str::from_utf8(&body_any.value);
                if payload_str.is_err() {
                    error!(
                        "can not convert to target type {}, this payload can not convert to string as well",
                        T::identity()
                    );
                    return Err(error);
                }
                let payload_str = payload_str.expect(
                    "Payload string should exist after checking utf8 conversion is not error",
                );
                warn!(
                    "payload {} can not convert to {} occur an error:{:?}",
                    payload_str,
                    T::identity(),
                    error
                );
                let ret: Result<ErrorResponse> = ErrorResponse::from_proto_any(&body_any);
                if let Err(e) = ret {
                    error!("trying convert to ErrorResponse occur an error:{:?}", e);
                    return Err(error);
                }
                let error_response = ret.expect(
                    "Error response should exist after checking deserialize result is not error",
                );
                return Err(ErrResponse(
                    error_response.request_id,
                    error_response.result_code,
                    error_response.error_code,
                    error_response.message,
                ));
            }
            de_body = ret.expect("Deserialize body should exist after checking it's not error");
        }

        Ok(GrpcMessage {
            headers,
            body: de_body,
            client_ip,
        })
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
        let byte_data = serde_json::to_vec(self);
        if let Err(error) = byte_data {
            return Err(Serialization(error));
        }
        any.value =
            byte_data.expect("Byte data should exist after checking serialize result is not error");
        Ok(any)
    }

    fn from_proto_any<T: GrpcMessageData>(any: &Any) -> Result<T> {
        let body: serde_json::Result<T> = serde_json::from_slice(&any.value);
        if let Err(error) = body {
            return Err(Serialization(error));
        };
        let body = body
            .expect("Deserialize body should exist after checking serialize result is not error");
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
