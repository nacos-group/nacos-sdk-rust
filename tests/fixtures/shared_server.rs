// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Shared test server fixture.
//!
//! Provides a singleton Nacos server instance that is started once and shared
//! across all integration tests. This eliminates the overhead of starting/stopping
//! a server for each individual test. Supports concurrent test execution.

use std::sync::Arc;
use tokio::sync::OnceCell;

use super::{NacosServerEnum, ServerMode};

/// Default port for shared rnacos server (standard Nacos port).
pub const SHARED_SERVER_HTTP_PORT: u16 = 8848;

/// Global shared server state - uses OnceCell for safe concurrent initialization.
static SHARED_SERVER: OnceCell<Arc<tokio::sync::Mutex<Option<NacosServerEnum>>>> =
    OnceCell::const_new();

/// Get or initialize the shared server state.
async fn get_or_init_server() -> &'static Arc<tokio::sync::Mutex<Option<NacosServerEnum>>> {
    SHARED_SERVER
        .get_or_init(|| async {
            let mode = ServerMode::default();
            eprintln!("[SharedServer] Initializing with mode: {:?}", mode);

            let server = match mode {
                ServerMode::Rnacos => NacosServerEnum::Rnacos(super::rnacos::RnacosServer::new(
                    SHARED_SERVER_HTTP_PORT,
                )),
                ServerMode::Docker => NacosServerEnum::Docker(
                    super::docker_nacos::DockerNacosServer::new(SHARED_SERVER_HTTP_PORT),
                ),
                ServerMode::ExternallyManaged => NacosServerEnum::ExternallyManaged(
                    super::externally_managed::ExternallyManagedServer::new(
                        super::DEFAULT_DOCKER_HTTP_PORT,
                    ),
                ),
            };

            Arc::new(tokio::sync::Mutex::new(Some(server)))
        })
        .await
}

/// Get the shared server address.
///
/// If the server hasn't been started yet, this will start it lazily.
/// The server will remain running for the duration of the test process.
/// Thread-safe and supports concurrent test execution.
pub async fn get_shared_server_addr() -> String {
    let state = get_or_init_server().await;
    let mut guard = state.lock().await;

    if let Some(ref mut server) = *guard {
        let needs_start = match server {
            NacosServerEnum::Rnacos(s) => !s.is_running(),
            NacosServerEnum::Docker(s) => !s.is_running(),
            NacosServerEnum::ExternallyManaged(s) => !s.is_running(),
        };

        if needs_start {
            eprintln!(
                "[SharedServer] Starting server on port {}...",
                server.server_addr()
            );
            server.start().await.expect("failed to start shared server");
            eprintln!("[SharedServer] Server started at {}", server.server_addr());
        }

        server.server_addr()
    } else {
        panic!("shared server not initialized")
    }
}

/// Get the server mode being used.
pub fn get_server_mode() -> ServerMode {
    ServerMode::default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_shared_server_mode_default() {
        let mode = super::get_server_mode();
        eprintln!("Default server mode: {:?}", mode);
    }
}
