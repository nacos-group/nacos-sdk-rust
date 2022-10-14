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
use tracing::{error, info};

pub mod request;
pub mod response;

#[derive(Debug)]
pub(crate) struct GrpcMessage<T>
where
    T: GrpcMessageBody,
{
    headers: HashMap<String, String>,
    body: T,
    client_ip: String,
}

impl<T> GrpcMessage<T>
where
    T: GrpcMessageBody,
{
    pub(crate) fn body(&self) -> &T {
        &self.body
    }

    pub(crate) fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub(crate) fn client_ip(&self) -> &String {
        &self.client_ip
    }

    pub(crate) fn to_payload(self) -> Result<Payload> {
        let mut payload = Payload::default();
        let mut meta_data = Metadata::default();
        meta_data.r#type = T::identity().to_string();
        meta_data.client_ip = self.client_ip.to_string();
        meta_data.headers = self.headers;

        let body = self.body.to_proto_any();
        if body.is_err() {
            let error = body.unwrap_err();
            error!("Serialize GrpcMessageBody occur an error: {:?}", error);
            return Err(error);
        }
        let body = body.unwrap();

        payload.metadata = Some(meta_data);
        payload.body = Some(body);
        Ok(payload)
    }

    pub(crate) fn from_payload(payload: Payload) -> Result<Self> {
        info!("from payload: {:?}", payload);
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

pub(crate) trait GrpcMessageBody:
    Debug + Clone + Serialize + DeserializeOwned + Send
{
    fn identity<'a>() -> std::borrow::Cow<'a, str>;

    fn to_proto_any(&self) -> Result<Any> {
        let mut any = Any::default();
        any.type_url = Self::identity().to_string();
        let byte_data = serde_json::to_vec(self);
        if let Err(error) = byte_data {
            return Err(Serialization(error));
        }
        any.value = byte_data.unwrap();
        Ok(any)
    }

    fn from_proto_any<T: GrpcMessageBody>(any: Any) -> Result<T> {
        let body: serde_json::Result<T> = serde_json::from_slice(&any.value);
        if let Err(error) = body {
            return Err(Serialization(error));
        };
        let body = body.unwrap();
        Ok(body)
    }
}

pub(crate) struct GrpcMessageBuilder<T>
where
    T: GrpcMessageBody,
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
    T: GrpcMessageBody,
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