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
//! - If you need async API, which can be enabled via `features = ["async"]`
//! ```toml
//! [dependencies]
//! nacos-sdk = { version = "0.3", features = ["default"] }
//! ```
//!
//! ## General Configurations and Initialization
//!
//! Nacos needs to be initialized. Please see the `api` module.
//!
//! ### Example of Config
//!
//! ```ignore
//!  let config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
//!        nacos_sdk::api::props::ClientProps::new()
//!           .server_addr("0.0.0.0:8848")
//!           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
//!           .namespace("")
//!           .app_name("todo-your-app-name"),
//!   )
//!   .build()?;
//! ```
//!
//! ### Example of Naming
//!
//! ```ignore
//!  let naming_service = nacos_sdk::api::naming::NamingServiceBuilder::new(
//!        nacos_sdk::api::props::ClientProps::new()
//!           .server_addr("0.0.0.0:8848")
//!           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
//!           .namespace("")
//!           .app_name("todo-your-app-name"),
//!   )
//!   .build()?;
//! ```
//!

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::path::Path;

const ENV_CONFIG_FILE_PATH: &str = "CONFIG_FILE_PATH";

lazy_static! {
    static ref PROPERTIES: HashMap<String, String> = {
        let env_file_path = std::env::var(ENV_CONFIG_FILE_PATH).ok();
        let _ = env_file_path.as_ref().map(|file_path| {
            dotenvy::from_path(Path::new(file_path)).or_else(|e| {
                let _ = dotenvy::dotenv();
                Err(e)
            })
        });
        dotenvy::dotenv().ok();

        let prop = dotenvy::vars()
            .into_iter()
            .collect::<HashMap<String, String>>();

        prop
    };
}

pub(crate) mod properties {
    use crate::PROPERTIES;

    pub(crate) fn get_value<Key, Default>(key: Key, default: Default) -> String
    where
        Key: AsRef<str>,
        Default: AsRef<str>,
    {
        PROPERTIES
            .get(key.as_ref())
            .map_or(default.as_ref().to_string(), |value| value.to_string())
    }

    pub(crate) fn get_value_u32<Key>(key: Key, default: u32) -> u32
    where
        Key: AsRef<str>,
    {
        PROPERTIES.get(key.as_ref()).map_or(default, |value| {
            value
                .to_string()
                .parse::<u32>()
                .map_or_else(|_e| default, |v| v)
        })
    }
}

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
            payload.metadata.unwrap().r#type,
            "com.alibaba.nacos.api.naming.remote.request.ServiceQueryRequest"
        );
        assert_eq!(
            payload.body.unwrap().value,
            Vec::from("{\"cluster\":\"DEFAULT\",\"healthyOnly\":true}")
        );
    }
}

#[cfg(test)]
mod test_config {
    use std::sync::Once;

    use tracing::metadata::LevelFilter;

    static LOGGER_INIT: Once = Once::new();

    pub(crate) fn setup_log() {
        LOGGER_INIT.call_once(|| {
            tracing_subscriber::fmt()
                .with_thread_names(true)
                .with_file(true)
                .with_level(true)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_max_level(LevelFilter::DEBUG)
                .init()
        });
    }
}
