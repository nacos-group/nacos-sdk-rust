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
/// error learn from Skywalking Rust.

/// Nacos Sdk Rust Result.
pub type Result<T> = std::result::Result<T, Error>;

/// Nacos Sdk Rust Error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Deserialization failed: {0}")]
    Deserialization(String),

    #[error("remote client shutdown failed: {0}")]
    ClientShutdown(String),

    /*
    #[error("tonic transport failed: {0}")]
    TonicTransport(#[from] tonic::transport::Error),

    #[error("tonic status: {0}")]
    TonicStatus(#[from] tonic::Status),
    */
    #[error("tokio task join failed: {0}")]
    TokioJoin(#[from] tokio::task::JoinError),

    #[error("tokio oneshot receive failed: {0}")]
    TokioOneshotRecv(#[from] tokio::sync::oneshot::error::RecvError),
}
