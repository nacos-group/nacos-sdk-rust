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
//! - If you need sync API, maybe `futures::executor::block_on(future_fn)`
//! ```toml
//! [dependencies]
//! nacos-sdk = { version = "0.4", features = ["default"] }
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
//!           .server_addr("127.0.0.1:8848")
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
//!           .server_addr("127.0.0.1:8848")
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

const ENV_NACOS_CLIENT_PROPS_FILE_PATH: &str = "NACOS_CLIENT_PROPS_FILE_PATH";

lazy_static! {
    static ref PROPERTIES: HashMap<String, String> = {
        let env_file_path = std::env::var(ENV_NACOS_CLIENT_PROPS_FILE_PATH).ok();
        let _ = env_file_path.as_ref().map(|file_path| {
            dotenvy::from_path(Path::new(file_path)).map_err(|e| {
                let _ = dotenvy::dotenv();
                e
            })
        });
        dotenvy::dotenv().ok();

        dotenvy::vars().collect::<HashMap<String, String>>()
    };
}

pub(crate) mod properties {
    use crate::PROPERTIES;

    pub(crate) fn get_value_option<Key>(key: Key) -> Option<String>
    where
        Key: AsRef<str>,
    {
        PROPERTIES.get(key.as_ref()).cloned()
    }

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
            value.to_string().parse::<u32>().unwrap_or(default)
        })
    }

    pub(crate) fn get_value_bool<Key>(key: Key, default: bool) -> bool
    where
        Key: AsRef<str>,
    {
        PROPERTIES.get(key.as_ref()).map_or(default, |value| {
            value.to_string().parse::<bool>().unwrap_or(default)
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
mod nacos_proto {
    pub mod v2 {
        tonic::include_proto!("_");
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
mod test_props {
    use crate::api::constants::ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION;
    use crate::properties::{get_value, get_value_bool, get_value_u32};

    #[test]
    fn test_get_value() {
        let v = get_value("ENV_TEST", "TEST");
        assert_eq!(v, "TEST");
    }

    #[test]
    fn test_get_value_bool() {
        let v = get_value_bool(ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION, true);
        assert_eq!(v, true);
    }

    #[test]
    fn test_get_value_u32() {
        let not_exist_key = "MUST_NOT_EXIST";
        let v = get_value_u32(not_exist_key, 91);
        assert_eq!(v, 91);
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
