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

use tower::BoxError;

/// Nacos Sdk Rust Result.
pub type Result<T> = std::result::Result<T, Error>;

/// Nacos Sdk Rust Error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("get result failed: {0}")]
    ErrResult(String),

    #[error("param:{0} with error_message:{1}")]
    InvalidParam(String, String),

    #[error("request_id:{0:?} ret_code:{1} error_code:{2} message:{3:?}")]
    ErrResponse(Option<String>, i32, i32, Option<String>),

    /// Config not found.
    #[cfg(feature = "config")]
    #[error("config not found: {0}")]
    ConfigNotFound(String),

    /// Config query conflict, it is being modified, please try later.
    #[cfg(feature = "config")]
    #[error("config query conflict: {0}")]
    ConfigQueryConflict(String),

    #[error("remote client shutdown failed: {0}")]
    ClientShutdown(String),

    #[error("remote client unhealthy failed: {0}")]
    ClientUnhealthy(String),

    #[error("tonic grpc transport error: {0}")]
    TonicGrpcTransport(#[from] tonic::transport::Error),

    #[error("tonic grpc status error: {0}")]
    TonicGrpcStatus(#[from] tonic::Status),

    #[error("grpc request error: {0}")]
    GrpcBufferRequest(#[from] BoxError),

    #[error("no available server")]
    NoAvailableServer,

    #[error("Wrong server address: {0}")]
    WrongServerAddress(String),


    #[error("Exceeded maximum retry attempts: {0}")]
    MaxRetriesExceeded(i32),
}
