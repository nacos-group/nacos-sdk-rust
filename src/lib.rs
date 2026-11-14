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

#![deny(unsafe_code)]
#![deny(rust_2018_idioms, clippy::disallowed_methods, clippy::disallowed_types)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

//! # Nacos in Rust
//!
//! Thorough examples have been provided in our [repository](https://github.com/nacos-group/nacos-sdk-rust).
//!
//! ## Add Dependency
//!
//! Add the dependency in `Cargo.toml`:
//! ```toml
//! [dependencies]
//! nacos-sdk = { version = "0.8", features = ["default"] }
//! ```
//!
//! ## General Configurations and Initialization
//!
//! Nacos needs to be initialized. Please see the `api` module.
//!
//! ### Example of Config
//!
//! ```no_run
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
//!       nacos_sdk::api::props::ClientProps::new()
//!          .server_addr("127.0.0.1:8848")
//!          // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
//!          .namespace("")
//!          .app_name("todo-your-app-name"),
//!  )
//!  .build()
//!  .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Example of Naming
//!
//! ```no_run
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let naming_service = nacos_sdk::api::naming::NamingServiceBuilder::new(
//!       nacos_sdk::api::props::ClientProps::new()
//!          .server_addr("127.0.0.1:8848")
//!          // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
//!          .namespace("")
//!          .app_name("todo-your-app-name"),
//!  )
//!  .build()
//!  .await?;
//! # Ok(())
//! # }
//! ```
//!

/// Nacos API
pub mod api;

mod common;
#[cfg(feature = "config")]
mod config;
#[cfg(feature = "naming")]
mod naming;

#[allow(dead_code)]
#[path = ""]
mod nacos_proto {
    #[path = "_.rs"]
    pub mod v2;
}

use crate::api::constants::ENV_NACOS_CLIENT_PROPS_FILE_PATH;
use std::collections::HashMap;

static PROPERTIES: std::sync::LazyLock<HashMap<String, String>> = std::sync::LazyLock::new(|| {
    let env_file_path = std::env::var(ENV_NACOS_CLIENT_PROPS_FILE_PATH).ok();
    let _ = env_file_path.as_ref().map(|file_path| {
        dotenvy::from_path(std::path::Path::new(file_path)).inspect_err(|_e| {
            let _ = dotenvy::dotenv();
        })
    });
    dotenvy::dotenv().ok();

    dotenvy::vars().collect::<HashMap<String, String>>()
});

pub(crate) mod properties {
    use crate::PROPERTIES;

    pub(crate) fn get_value_option<Key>(key: Key) -> Option<String>
    where
        Key: AsRef<str>,
    {
        PROPERTIES.get(key.as_ref()).cloned()
    }

    pub(crate) fn get_value_u16<Key>(key: Key, default: u16) -> u16
    where
        Key: AsRef<str>,
    {
        PROPERTIES
            .get(key.as_ref())
            .map_or(default, |value| value.parse::<u16>().unwrap_or(default))
    }

    pub(crate) fn get_value_bool<Key>(key: Key, default: bool) -> bool
    where
        Key: AsRef<str>,
    {
        PROPERTIES
            .get(key.as_ref())
            .map_or(default, |value| value.parse::<bool>().unwrap_or(default))
    }
}

#[cfg(test)]
mod tests {
    use prost_types::Any;
    use std::collections::HashMap;

    use crate::nacos_proto::v2::Metadata;
    use crate::nacos_proto::v2::Payload;

    #[test]
    fn it_works_nacos_proto() {
        let body = Any {
            type_url: String::new(),
            value: Vec::from("{\"cluster\":\"DEFAULT\",\"healthyOnly\":true}"),
        };
        let metadata = Metadata {
            r#type: String::from("com.alibaba.nacos.api.naming.remote.request.ServiceQueryRequest"),
            client_ip: String::from("127.0.0.1"),
            headers: HashMap::new(),
        };
        let payload = Payload {
            metadata: Some(metadata),
            body: Some(body),
        };
        // println!("{:?}", payload);
        assert_eq!(
            payload
                .metadata
                .expect("Metadata should exist after checking it's some")
                .r#type,
            "com.alibaba.nacos.api.naming.remote.request.ServiceQueryRequest"
        );
        assert_eq!(
            payload
                .body
                .expect("Body should exist after checking it's some")
                .value,
            Vec::from("{\"cluster\":\"DEFAULT\",\"healthyOnly\":true}")
        );
    }
}

#[cfg(test)]
mod test_props {
    use crate::api::constants::ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION;
    use crate::properties::{get_value_bool, get_value_u16};

    #[test]
    fn test_get_value_bool() {
        let v = get_value_bool(ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION, true);
        assert!(v);
    }

    #[test]
    fn test_get_value_u16() {
        let not_exist_key = "MUST_NOT_EXIST";
        let v = get_value_u16(not_exist_key, 91);
        assert_eq!(v, 91);
    }

    /// `PROPERTIES` is a process-wide snapshot, so env driven cases run in child processes,
    /// one child per `NACOS_CLIENT_TEST_PARSE_CASE` value.
    #[test]
    fn test_get_value_parse_from_env() {
        const CHILD_CASE_ENV: &str = "NACOS_CLIENT_TEST_PARSE_CASE";
        const PORT_ENV: &str = "NACOS_CLIENT_TEST_PARSE_PORT";
        const BOOL_ENV: &str = "NACOS_CLIENT_TEST_PARSE_BOOL";

        let Ok(case) = std::env::var(CHILD_CASE_ENV) else {
            for (case, port, bool_value) in [
                ("valid", "1234", "false"),
                ("invalid", "not-a-port", "not-a-bool"),
                ("overflow", "70000", "TRUE"),
            ] {
                crate::test_config::run_child_test(
                    "test_props::test_get_value_parse_from_env",
                    &[
                        (CHILD_CASE_ENV, case),
                        (PORT_ENV, port),
                        (BOOL_ENV, bool_value),
                    ],
                    &[],
                );
            }
            return;
        };

        match case.as_str() {
            "valid" => {
                assert_eq!(get_value_u16(PORT_ENV, 91), 1234);
                assert!(!get_value_bool(BOOL_ENV, true));
            }
            // unparseable values fall back to the default
            "invalid" => {
                assert_eq!(get_value_u16(PORT_ENV, 91), 91);
                assert!(get_value_bool(BOOL_ENV, true));
            }
            // out-of-range u16 falls back; bool parsing is case-sensitive
            "overflow" => {
                assert_eq!(get_value_u16(PORT_ENV, 91), 91);
                assert!(!get_value_bool(BOOL_ENV, false));
            }
            other => panic!("unknown test case: {other}"),
        }
    }
}

#[cfg(test)]
mod test_config {
    use std::sync::Once;

    use tracing::metadata::LevelFilter;

    static LOGGER_INIT: Once = Once::new();

    pub(crate) fn setup_log() {
        LOGGER_INIT.call_once(|| {
            let _ = tracing_subscriber::fmt()
                .with_thread_names(true)
                .with_file(true)
                .with_level(true)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_max_level(LevelFilter::INFO)
                .try_init();
        });
    }

    /// Runs a test with the logging initialized.
    pub(crate) fn run_test<T, F>(test: F) -> T
    where
        F: FnOnce() -> T,
    {
        setup_log();
        test()
    }

    /// Waits until `path` exists on disk, failing the test on timeout.
    ///
    /// The cache writes to disk in the background, so a test that wants to
    /// inspect the file has to wait for it to appear.
    pub(crate) async fn wait_for_file(path: &std::path::Path, msg: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !path.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect(msg);
    }

    /// Waits until `flag` is set, returning whether it was set in time.
    ///
    /// Listener callbacks run on a background thread, so a test has to wait
    /// for the callback to have run.
    pub(crate) async fn wait_for_flag(
        flag: &std::sync::atomic::AtomicBool,
        timeout: std::time::Duration,
    ) -> bool {
        tokio::time::timeout(timeout, async {
            while !flag.load(std::sync::atomic::Ordering::Acquire) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    /// Runs a test in a child process with the given environment.
    ///
    /// Used by tests that need a fresh process, because the properties are
    /// read once into a process-wide snapshot.
    pub(crate) fn run_child_test(test: &str, envs: &[(&str, &str)], remove: &[&str]) {
        let current_exe =
            std::env::current_exe().expect("current test executable should be available");
        let mut command = std::process::Command::new(current_exe);
        command.args(["--exact", test]).envs(envs.iter().copied());
        for key in remove {
            command.env_remove(key);
        }
        let status = command.status().expect("child test should run");
        assert!(status.success(), "child test {test} failed");
    }
}
