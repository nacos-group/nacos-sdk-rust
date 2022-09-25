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

#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

//! # Nacos in Rust
//!
//! Thorough examples have been provided in our [repository](https://github.com/nacos-group/nacos-sdk-rust).
//!
//! ## Add Dependency
//!
//! Add the dependency in `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! nacos-client = { version = "0.1.0", features = ["default"] }
//! ```
//!
//! ## General Configurations and Initialization
//!
//! Nacos needs to be initialized. Please see the `api` module.
//!
//! ### Example of Config
//!
//! ```rust
//!  let mut config_service = nacos_client::api::config::ConfigServiceBuilder::new(
//!        nacos_client::api::props::ClientProps::new()
//!           .server_addr("0.0.0.0:9848")
//!           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
//!           .namespace("")
//!           .app_name("todo-your-app-name"),
//!   )
//!   .build()//.await
//!   ;
//! ```
//!

/// Nacos API
pub mod api;

mod common;
#[cfg(feature = "config")]
mod config;
#[cfg(feature = "naming")]
mod naming;

mod nacos_proto {
    pub mod v2 {
        include!("_.rs");
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
