use std::collections::HashMap;

use tonic::async_trait;

pub mod remote_client;

#[async_trait]
pub(crate) trait RemoteClient {}
