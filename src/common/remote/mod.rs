use tonic::async_trait;

pub(crate) mod remote_client;
pub(crate) mod request;
pub(crate) mod response;

#[async_trait]
pub(crate) trait RemoteClient {}

pub(crate) trait PayloadConverter {
    fn convert_to_grpc_payload() -> crate::nacos_proto::v2::Payload;
}
