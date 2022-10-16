// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

/// Nacos Sdk Rust Result.
pub type Result<T> = std::result::Result<T, Error>;

/// Nacos Sdk Rust Error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Deserialization failed: {0}")]
    Deserialization(String),

    #[error("get result failed: {0}")]
    ErrResult(String),

    /// Config not found.
    #[error("config not exist: {0}")]
    ConfigNotFound(String),

    #[error("remote client shutdown failed: {0}")]
    ClientShutdown(String),

    #[error("grpcio conn failed: {0}")]
    GrpcioJoin(#[from] grpcio::Error),

    #[error("tokio task join failed: {0}")]
    TokioJoin(#[from] tokio::task::JoinError),

    #[error("tokio oneshot receive failed: {0}")]
    TokioOneshotRecv(#[from] tokio::sync::oneshot::error::RecvError),

    #[error("grpc payload metadata is empty")]
    GrpcPayloadMetaDataEmpty,

    #[error("grpc payload body is empty")]
    GrpcPayloadBodyEmpty,

    #[error("No response returned")]
    ServerNoResponse,

    #[error("naming service register service failed: resultCode: {0}, errorCode:{1}, message:{2}")]
    NamingRegisterServiceFailed(i32, i32, String),

    #[error(
        "naming service deregister service failed: resultCode: {0}, errorCode:{1}, message:{2}"
    )]
    NamingDeregisterServiceFailed(i32, i32, String),

    #[error("naming service batch register services failed: resultCode: {0}, errorCode:{1}, message:{2}")]
    NamingBatchRegisterServiceFailed(i32, i32, String),
}
