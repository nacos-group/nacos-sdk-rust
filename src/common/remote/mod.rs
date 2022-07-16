use tonic::async_trait;

pub(crate) mod remote_client;
pub(crate) mod request;

#[async_trait]
pub(crate) trait RemoteClient {}
