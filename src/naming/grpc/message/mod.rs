use std::{collections::HashMap, error::Error};

use prost_types::Any;
use serde::{Serialize,de::DeserializeOwned};


use crate::nacos_proto::v2::{Payload, Metadata};
use tracing::error;
use std::fmt::Debug;

#[derive(Debug)]
pub(crate) struct GrpcMessage<T> 
where
 T:  GrpcMessageBody
{
    headers: HashMap<String, String>,
    body: T,
    client_ip: String,
}

impl<T > GrpcMessage<T>
where 
    T: GrpcMessageBody
{
    
    pub fn to_payload(self) -> Option<Payload> {
        let mut payload = Payload::default();
        let mut meta_data = Metadata::default();
        meta_data.r#type = self.body.request_type().to_string();
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

    pub fn from_payload(payload: Payload) -> Option<Self> {
       let meta_data = payload.metadata?;
       let body = payload.body?;
       let body = T::from_proto_any(body);
       if body.is_err() {
        let err = body.unwrap_err();
        error!("Deserialize from Any occur an error:{:?}", err);
        return None;
       }
       let body = body.unwrap();
       let client_ip = meta_data.client_ip;
       let headers = meta_data.headers;

       Some(GrpcMessage {
        headers,
        body,
        client_ip
       })
    }
}

pub(crate) trait GrpcMessageBody: Debug + Clone + Serialize + DeserializeOwned {

    fn request_type(&self) -> std::borrow::Cow<'_, str>;

    fn to_proto_any(&self) -> Result<Any, Box<dyn Error>> {
        let mut any = Any::default();
        any.type_url = self.request_type().to_string();
        let byte_data = serde_json::to_vec(self)?;
        any.value = byte_data;
        Ok(any)
    }

    fn from_proto_any<T: GrpcMessageBody>(any: Any) -> Result<T, Box<dyn Error>> {
        let body = any.value;
        let body: serde_json::Result<T> = serde_json::from_slice(&body);

        if let Err(error) = body {
            return Err(Box::new(error));
        }
        let body = body.unwrap();
        Ok(body)
    }
}

#[cfg(test)]
mod tests {

    use nacos_macro::GrpcMessageBody;
    use serde::Deserialize;
    use super::*;


    #[derive(Clone, Debug, Serialize, Deserialize, GrpcMessageBody)]
    #[message_attr(request_type = "queryInstance")]
    struct QueryInstanceRequest {
        request_type: String,
        service_name: String,
        ip: String,
        port: i32
    }

    #[test]
    fn test_serialize_and_deserialize_message() {
        let body = QueryInstanceRequest {
            request_type: "query_instance_request".to_string(),
            service_name: "query_instance".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 8848
        };

        let grpc_message: GrpcMessage<QueryInstanceRequest> = GrpcMessage {
            headers: HashMap::<String, String>::new(),
            body,
            client_ip: "127.0.0.1".to_string()
        };

        let payload = grpc_message.to_payload();

        println!("payload content: {:?}", payload);

        let payload = payload.unwrap();

        let request = GrpcMessage::<QueryInstanceRequest>::from_payload(payload);

        let request = request.unwrap();

        println!("deserialize message from payload: {:?}", request)

    }

}