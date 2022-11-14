use lazy_static::lazy_static;
use prost_types::Any;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;

use crate::{
    api::error::Error::GrpcPayloadBodyEmpty,
    api::error::Error::GrpcPayloadMetaDataEmpty,
    api::error::Error::Serialization,
    api::error::Result,
    nacos_proto::v2::{Metadata, Payload},
};
use std::fmt::Debug;
use tracing::{debug, error};

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
        debug!("from payload: {:?}", payload);
        let meta_data = payload.metadata;
        if meta_data.is_none() {
            return Err(GrpcPayloadMetaDataEmpty);
        }
        let meta_data = meta_data.unwrap();

        let body = payload.body;
        if body.is_none() {
            return Err(GrpcPayloadBodyEmpty);
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
        let client_ip = meta_data.client_ip;
        let headers = meta_data.headers;

        Ok(GrpcMessage {
            headers,
            body,
            client_ip,
        })
    }

    pub(crate) fn unwrap_all(self) -> (T, HashMap<String, String>, String) {
        (self.body, self.headers, self.client_ip)
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

    pub(crate) fn build(self) -> GrpcMessage<T> {
        GrpcMessage {
            headers: self.headers,
            body: self.body,
            client_ip: self.client_ip,
        }
    }
}
