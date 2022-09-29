use lazy_static::lazy_static;
use prost_types::Any;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;

use crate::{
    nacos_proto::v2::{Metadata, Payload},
    naming::{
        error::{ErrorKind, NacosError},
        grpc::message::response::ErrorResponse,
    },
};
use std::fmt::Debug;
use tracing::{error, info};

pub mod request;
pub mod response;

#[derive(Debug)]
pub struct GrpcMessage<T>
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
    pub fn to_payload(self) -> Option<Payload> {
        let mut payload = Payload::default();
        let mut meta_data = Metadata::default();
        meta_data.r#type = T::type_url().to_string();
        meta_data.client_ip = self.client_ip.to_string();
        meta_data.headers = self.headers;

        let body = self.body.to_proto_any();
        if body.is_err() {
            let error = body.unwrap_err();
            error!("Serialize GrpcMessageBody occur an error: {:?}", error);
            return None;
        }
        let body = body.unwrap();

        payload.metadata = Some(meta_data);
        payload.body = Some(body);
        Some(payload)
    }

    pub fn from_payload(payload: Payload) -> Result<Self, NacosError> {
        info!("from payload: {:?}", payload);
        let meta_data = payload.metadata;
        if meta_data.is_none() {
            return Err(NacosError {
                kind: ErrorKind::NoneMetaDataError,
            });
        }
        let meta_data = meta_data.unwrap();
        let type_url = meta_data.r#type;

        let body = payload.body;
        if body.is_none() {
            return Err(NacosError {
                kind: ErrorKind::NonePayloadBody,
            });
        }

        let body = body.unwrap();

        if type_url == ErrorResponse::type_url() {
            let error = Self::response_to_error(body);
            return Err(error);
        }

        let body = T::from_proto_any(body);
        if let Err(error) = body {
            error!(
                "Deserialize from Any to GrpcMessage occur an error:{:?}",
                error
            );
            return Err(NacosError {
                kind: ErrorKind::SerdeJsonError(error),
            });
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

    pub fn response_to_error(body: Any) -> NacosError {
        let error_rsp = ErrorResponse::from_proto_any::<ErrorResponse>(body);
        if let Err(error) = error_rsp {
            let kind = ErrorKind::SerdeJsonError(error);
            let nacos_error = NacosError { kind };
            return nacos_error;
        }
        let error_rsp = error_rsp.unwrap();

        let kind = ErrorKind::ResponseError {
            result_code: error_rsp.result_code,
            error_code: error_rsp.error_code,
            message: error_rsp.message,
        };

        NacosError { kind }
    }

    pub fn unwrap_all(self) -> (T, HashMap<String, String>, String) {
        (self.body, self.headers, self.client_ip)
    }
}

pub trait GrpcMessageBody: Debug + Clone + Serialize + DeserializeOwned + Send {
    fn type_url<'a>() -> std::borrow::Cow<'a, str>;

    fn to_proto_any(&self) -> Result<Any, serde_json::Error> {
        let mut any = Any::default();
        any.type_url = Self::type_url().to_string();
        let byte_data = serde_json::to_vec(self)?;
        any.value = byte_data;
        Ok(any)
    }

    fn from_proto_any<T: GrpcMessageBody>(any: Any) -> Result<T, serde_json::Error> {
        let body: serde_json::Result<T> = serde_json::from_slice(&any.value);
        let body = body?;
        Ok(body)
    }
}

pub struct GrpcMessageBuilder<T>
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
    pub fn new(body: T) -> Self {
        GrpcMessageBuilder {
            headers: HashMap::<String, String>::new(),
            body,
            client_ip: LOCAL_IP.to_owned(),
        }
    }

    pub fn header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    pub fn build(self) -> GrpcMessage<T> {
        GrpcMessage {
            headers: self.headers,
            body: self.body,
            client_ip: self.client_ip,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use nacos_macro::GrpcMessageBody;
    use serde::Deserialize;

    #[derive(Clone, Debug, Serialize, Deserialize, GrpcMessageBody)]
    #[message_attr(request_type = "queryInstance")]
    struct QueryInstanceRequest {
        request_type: String,
        service_name: String,
        ip: String,
        port: i32,
    }

    #[test]
    fn test_serialize_and_deserialize_message() {
        let body = QueryInstanceRequest {
            request_type: "query_instance_request".to_string(),
            service_name: "query_instance".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 8848,
        };

        let grpc_message: GrpcMessage<QueryInstanceRequest> = GrpcMessage {
            headers: HashMap::<String, String>::new(),
            body,
            client_ip: "127.0.0.1".to_string(),
        };

        let payload = grpc_message.to_payload();

        println!("payload content: {:?}", payload);

        let payload = payload.unwrap();

        let request = GrpcMessage::<QueryInstanceRequest>::from_payload(payload);

        let request = request.unwrap();

        println!("deserialize message from payload: {:?}", request)
    }
}
