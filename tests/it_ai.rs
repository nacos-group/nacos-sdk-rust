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

//! Integration tests for the AI (MCP + Prompt) API.
//!
//! Requires Nacos Server 3.x; the default 2.5.4 profile skips these tests.

#![allow(dead_code)]

mod fixtures;
mod shared;

#[cfg(feature = "ai")]
mod ai_integration_tests {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use nacos_sdk::api::ai::{
        AiService, AiServiceBuilder, McpEndpointSpec, McpResourceSpecification, McpServerBasicInfo,
        McpServerDetailInfo, McpServerListener, McpToolSpecification,
    };
    use nacos_sdk::api::props::ClientProps;

    use crate::fixtures::container_nacos::ServerProfile;
    use crate::fixtures::shared_server::get_shared_server_addr;

    fn skip_unless_nacos3() -> bool {
        if ServerProfile::from_env() == ServerProfile::Nacos3 {
            return false;
        }
        eprintln!(
            "[it_ai] skipping: requires Nacos 3.x, set NACOS_SERVER_PROFILE=3.3 and a 3.3 image"
        );
        true
    }

    fn unique_name(prefix: &str) -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_millis();
        format!("{prefix}-{millis}")
    }

    async fn create_ai_service(server_addr: &str) -> AiService {
        create_ai_service_with_interval(server_addr, Duration::from_secs(10)).await
    }

    fn basic_spec(name: &str) -> McpServerBasicInfo {
        basic_spec_with_version(name, "1.0.0", "rust sdk ai it")
    }

    fn remote_basic_spec(name: &str) -> McpServerBasicInfo {
        let service_ref = serde_json::json!({
            "namespaceId": "public",
            "groupName": "DEFAULT_GROUP",
            "serviceName": format!("{name}-svc")
        });
        serde_json::from_value(serde_json::json!({
            "name": name,
            "protocol": "http",
            "description": "rust sdk ai it",
            "versionDetail": {"version": "1.0.0", "is_latest": true},
            "remoteServerConfig": {
                "frontEndpointConfigList": [{
                    "endpointType": "REF",
                    "protocol": "http",
                    "path": "/mcp",
                    "endpointData": service_ref
                }]
            },
            "enabled": true
        }))
        .expect("remote basic spec should parse")
    }

    #[tokio::test]
    #[ignore]
    async fn test_publish_and_query_mcp_server() {
        crate::shared::setup_log();
        if skip_unless_nacos3() {
            return;
        }
        let server_addr = get_shared_server_addr().await;
        let service = create_ai_service(&server_addr).await;
        let name = unique_name("it-mcp");

        let tools: McpToolSpecification = serde_json::from_value(serde_json::json!({
            "tools": [{"name": "echo", "description": "Echo", "inputSchema": {"type": "object"}}]
        }))
        .expect("tools should parse");
        let resources: McpResourceSpecification = serde_json::from_value(serde_json::json!({
            "resources": [{"name": "note", "uri": "file:///tmp/note.txt"}]
        }))
        .expect("resources should parse");
        let endpoint: McpEndpointSpec = serde_json::from_value(serde_json::json!({
            "type": "DIRECT",
            "data": {"address": "127.0.0.1", "port": "18080"}
        }))
        .expect("endpoint should parse");

        let released = service
            .release_mcp_server(
                basic_spec(&name),
                Some(tools),
                Some(resources),
                Some(endpoint),
            )
            .await
            .expect("release should succeed");
        assert!(!released.is_empty());

        let detail = service
            .get_mcp_server(name.clone(), Some("1.0.0".to_string()))
            .await
            .expect("query should succeed");
        assert_eq!(detail.name.as_deref(), Some(name.as_str()));

        let latest = service
            .get_mcp_server(name.clone(), None)
            .await
            .expect("latest query should succeed");
        assert_eq!(
            latest.version_detail.as_ref().map(|v| v.version.as_str()),
            Some("1.0.0")
        );

        let duplicate = service
            .release_mcp_server(basic_spec(&name), None, None, None)
            .await
            .expect_err("duplicate release should fail");
        let message = format!("{duplicate}");
        assert!(
            message.contains("error_code=409") && message.contains("already exist"),
            "unexpected duplicate error: {message}"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_mcp_endpoint_register_and_deregister() {
        crate::shared::setup_log();
        if skip_unless_nacos3() {
            return;
        }
        let server_addr = get_shared_server_addr().await;
        let service = create_ai_service(&server_addr).await;
        let name = unique_name("it-mcp-endpoint");

        let endpoint: McpEndpointSpec = serde_json::from_value(serde_json::json!({
            "type": "REF",
            "data": {
                "namespaceId": "public",
                "groupName": "DEFAULT_GROUP",
                "serviceName": format!("{name}-svc")
            }
        }))
        .expect("endpoint should parse");

        service
            .release_mcp_server(remote_basic_spec(&name), None, None, Some(endpoint))
            .await
            .expect("release should succeed");

        service
            .register_mcp_server_endpoint(
                name.clone(),
                "127.0.0.1",
                18080,
                Some("1.0.0".to_string()),
            )
            .await
            .expect("register endpoint should succeed");

        let mut registered = false;
        for _ in 0..40 {
            if let Ok(detail) = service
                .get_mcp_server(name.clone(), Some("1.0.0".to_string()))
                .await
            {
                registered = detail
                    .backend_endpoints
                    .as_ref()
                    .map(|endpoints| endpoints.iter().any(|endpoint| endpoint.port == 18080))
                    .unwrap_or(false);
                if registered {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(registered, "backend endpoint should appear");

        service
            .deregister_mcp_server_endpoint(name.clone(), "127.0.0.1", 18080)
            .await
            .expect("deregister endpoint should succeed");

        let mut removed = false;
        for _ in 0..40 {
            if let Ok(detail) = service
                .get_mcp_server(name.clone(), Some("1.0.0".to_string()))
                .await
            {
                // `registered` required Some([...port]), so a null list here is a real removal.
                removed = detail
                    .backend_endpoints
                    .as_ref()
                    .map(|endpoints| endpoints.iter().all(|endpoint| endpoint.port != 18080))
                    .unwrap_or(true);
                if removed {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(removed, "backend endpoint should disappear");
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_prompt_not_found() {
        crate::shared::setup_log();
        if skip_unless_nacos3() {
            return;
        }
        let server_addr = get_shared_server_addr().await;
        let service = create_ai_service(&server_addr).await;
        let error = service
            .get_prompt("it-prompt-does-not-exist", None, Some("latest".to_string()))
            .await
            .expect_err("missing prompt should fail");
        let message = format!("{error}");
        assert!(
            message.contains("Prompt not found"),
            "unexpected error: {message}"
        );
        // Nacos 3.3 reports the missing prompt as the server status 404 on the
        // gRPC prompt API; the v2 business code 20004 is never sent.
        assert!(
            message.contains("error_code=404"),
            "unexpected error: {message}"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_release_mcp_server_draft() {
        crate::shared::setup_log();
        if skip_unless_nacos3() {
            return;
        }
        let server_addr = get_shared_server_addr().await;
        let service = create_ai_service(&server_addr).await;
        let name = unique_name("it-mcp-draft");

        // 3.3.0 advertises `mcpDraftRelease` unconditionally, but the draft API
        // is gated by the server-side LIFECYCLE_MANAGED cutover, which needs a
        // ready search projection (false in a searchless standalone). Either
        // outcome proves the client-side strict ability gate passed and the
        // request reached the server.
        match service
            .release_mcp_server_draft(basic_spec(&name), None, None, None)
            .await
        {
            Ok(released) => assert!(!released.is_empty()),
            Err(error) => {
                let message = format!("{error}");
                assert!(
                    message.contains("error_code=409")
                        && message.contains("LIFECYCLE_MANAGED cutover"),
                    "unexpected draft release error: {message}"
                );
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_missing_mcp_server_reports_404() {
        crate::shared::setup_log();
        if skip_unless_nacos3() {
            return;
        }
        let server_addr = get_shared_server_addr().await;
        let service = create_ai_service(&server_addr).await;
        let error = service
            .get_mcp_server(unique_name("it-mcp-missing"), None)
            .await
            .expect_err("missing mcp server should fail");
        let message = format!("{error}");
        assert!(
            message.contains("error_code=404"),
            "unexpected error: {message}"
        );
    }

    struct ChannelListener {
        tx: tokio::sync::mpsc::UnboundedSender<Arc<McpServerDetailInfo>>,
    }

    impl McpServerListener for ChannelListener {
        fn notify(&self, mcp_server: Arc<McpServerDetailInfo>) {
            let _ = self.tx.send(mcp_server);
        }
    }

    async fn create_ai_service_with_interval(server_addr: &str, interval: Duration) -> AiService {
        AiServiceBuilder::new(ClientProps::new().server_addr(server_addr))
            .mcp_server_update_interval(interval)
            .build()
            .await
            .expect("AiServiceBuilder should build")
    }

    fn basic_spec_with_version(name: &str, version: &str, description: &str) -> McpServerBasicInfo {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "protocol": "stdio",
            "description": description,
            "versionDetail": {"version": version, "is_latest": true},
            "localServerConfig": {"command": "echo", "args": ["it"]},
            "enabled": true
        }))
        .expect("basic spec should parse")
    }

    #[tokio::test]
    #[ignore]
    async fn test_subscribe_mcp_server_receives_new_latest() {
        crate::shared::setup_log();
        if skip_unless_nacos3() {
            return;
        }
        let server_addr = get_shared_server_addr().await;
        let service =
            create_ai_service_with_interval(&server_addr, Duration::from_millis(200)).await;
        let name = unique_name("it-mcp-sub");

        service
            .release_mcp_server(
                basic_spec_with_version(&name, "1.0.0", "first"),
                None,
                None,
                None,
            )
            .await
            .expect("first release should succeed");

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let listener = Arc::new(ChannelListener { tx });
        let subscribed = service
            .subscribe_mcp_server(name.clone(), None, listener.clone())
            .await
            .expect("subscribe should succeed")
            .expect("subscribed mcp server should be returned");
        assert_eq!(subscribed.description.as_deref(), Some("first"));
        let initial = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("initial notification")
            .expect("channel open");
        assert_eq!(initial.description.as_deref(), Some("first"));
        assert_eq!(
            initial
                .version_detail
                .as_ref()
                .map(|detail| detail.version.as_str()),
            Some("1.0.0")
        );

        service
            .release_mcp_server(
                basic_spec_with_version(&name, "1.1.0", "second"),
                None,
                None,
                None,
            )
            .await
            .expect("second release should succeed");

        let changed = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("change notification within a few poll intervals")
            .expect("channel open");
        assert_eq!(changed.description.as_deref(), Some("second"));
        assert_eq!(
            changed
                .version_detail
                .as_ref()
                .map(|detail| detail.version.as_str()),
            Some("1.1.0")
        );

        service
            .unsubscribe_mcp_server(name.clone(), None, listener.clone())
            .await
            .expect("unsubscribe should succeed");

        // The earlier change event proves the listener is wired up; this quiet window
        // validates the user-visible no-notification contract after unsubscribe (the
        // unit test covers the poller stop path). A closed channel yields `Ok(None)`,
        // which already fails `is_err()`; keeping `listener` alive prevents a false
        // failure rather than a false pass.
        service
            .release_mcp_server(
                basic_spec_with_version(&name, "1.2.0", "third"),
                None,
                None,
                None,
            )
            .await
            .expect("third release should succeed");
        assert!(
            tokio::time::timeout(Duration::from_millis(600), rx.recv())
                .await
                .is_err(),
            "listener must not be notified after unsubscribe"
        );
    }
}
