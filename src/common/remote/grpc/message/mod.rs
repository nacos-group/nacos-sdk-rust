use lazy_static::lazy_static;
use prost_types::Any;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;

use crate::api::error::Error::ErrResult;
use crate::api::error::Error::Serialization;
use crate::api::error::Result;
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
    pub(crate) fn body(&self) -> &T {
        &self.body
    }

    pub(crate) fn client_ip(&self) -> &str {
        &self.client_ip
    }

    pub(crate) fn headers(&self) -> &HashMap<String, String> {
        &self.headers
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
        let body = body.unwrap();

        payload.metadata = Some(meta_data);
        payload.body = Some(body);
        Ok(payload)
    }

    pub(crate) fn from_payload(payload: Payload) -> Result<Self> {
        let body = payload.body;
        if body.is_none() {
            return Err(ErrResult("grpc payload body is empty".to_string()));
        }

        let body = body.unwrap();

        let body = T::from_proto_any(body);
        if let Err(error) = body {
            error!(
                "Deserialize from Any to GrpcMessage occur an error:{:?}",
                error
            );
            return Err(error);
        }
        let body = body.unwrap();

        let client_ip;
        let headers;
        let meta_data = payload.metadata;

        if let Some(meta_data) = meta_data {
            client_ip = meta_data.client_ip;
            headers = meta_data.headers;
        } else {
            client_ip = "".to_string();
            headers = HashMap::new();
        }

        Ok(GrpcMessage {
            headers,
            body,
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
        any.value = byte_data.unwrap();
        Ok(any)
    }

    fn from_proto_any<T: GrpcMessageData>(any: Any) -> Result<T> {
        let body: serde_json::Result<T> = serde_json::from_slice(&any.value);
        if let Err(error) = body {
            return Err(Serialization(error));
        };
        let body = body.unwrap();
        Ok(body)
    }
}

pub(crate) trait GrpcRequestMessage: GrpcMessageData {
    fn header(&self, key: &str) -> Option<&String>;

    fn headers(&self) -> &HashMap<String, String>;

    fn take_headers(&mut self) -> HashMap<String, String>;

    fn add_headers(&mut self, map: HashMap<String, String>);

    fn request_id(&self) -> Option<&String>;

    fn module(&self) -> &str;
}

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

lazy_static! {
    static ref LOCAL_IP: String = local_ipaddress::get().unwrap();
}

impl<T> GrpcMessageBuilder<T>
where
    T: GrpcMessageData,
{
    pub(crate) fn new(body: T) -> Self {
        GrpcMessageBuilder {
            headers: HashMap::<String, String>::new(),
            body,
            client_ip: LOCAL_IP.to_owned(),
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

    pub(crate) fn client_ip(mut self, ip: String) -> Self {
        self.client_ip = ip;
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

#[cfg(test)]
mod tests {

    use nacos_macro::request;

    use super::*;

    #[request(identity = "TestGrpcRequestMessage", module = "internal")]
    struct TestGrpcRequestMessage {}

    #[test]
    pub fn test_grpc_message() {
        let mut grpc_message_builder =
            GrpcMessageBuilder::<TestGrpcRequestMessage>::new(TestGrpcRequestMessage {
                request_id: Some("grpc_message_id123".to_string()),
                ..Default::default()
            });

        let mut test_headers = HashMap::<String, String>::new();

        test_headers.insert(
            "grpc_headers_key".to_string(),
            "grpc_headers_value".to_string(),
        );

        grpc_message_builder = grpc_message_builder
            .client_ip(String::from("119.110.120.112"))
            .header("grpc_key".to_string(), "grpc_value".to_string())
            .headers(test_headers);

        let grpc_message = grpc_message_builder.build();

        assert_eq!(grpc_message.client_ip(), "119.110.120.112");

        let grpc_key_value_opt = grpc_message.headers().get("grpc_key");
        assert!(grpc_key_value_opt.is_some());
        let grpc_key_value = grpc_key_value_opt.unwrap();
        assert_eq!(grpc_key_value, "grpc_value");

        let grpc_headers_key_opt = grpc_message.headers().get("grpc_headers_key");
        assert!(grpc_headers_key_opt.is_some());
        let grpc_headers_key_value = grpc_headers_key_opt.unwrap();
        assert_eq!(grpc_headers_key_value, "grpc_headers_value");

        let payload = grpc_message.into_payload().unwrap();
        let metadata = payload.metadata.unwrap();
        let any_body = payload.body.unwrap();

        let metadata_client_ip = metadata.client_ip;
        assert_eq!(metadata_client_ip, "119.110.120.112");

        let headers = metadata.headers;
        assert_eq!(
            headers.get("grpc_headers_key").unwrap(),
            "grpc_headers_value"
        );
        assert_eq!(headers.get("grpc_key").unwrap(), "grpc_value");

        let metadata_type = metadata.r#type;
        assert_eq!(metadata_type, "TestGrpcRequestMessage");

        let type_url = any_body.type_url;
        assert_eq!(type_url, "TestGrpcRequestMessage");

        let any_body_data = any_body.value;

        let serde_json_data: serde_json::Result<TestGrpcRequestMessage> =
            serde_json::from_slice(&any_body_data);
        let grpc_message = serde_json_data.unwrap();

        let request_id = grpc_message.request_id.unwrap();
        assert_eq!(request_id, "grpc_message_id123".to_string());
    }
}
