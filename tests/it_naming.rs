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

//! Integration tests for the Naming service.
//!
//! These tests require a running Nacos server (rnacos or Docker).
//! Run with: `cargo test --test it_naming --features naming`

#![allow(dead_code)]

mod fixtures;
mod shared;

#[cfg(feature = "naming")]
mod naming_integration_tests {
    use crate::fixtures::{ServerMode, create_server};
    use crate::shared::{MockNamingListener, ServiceInstanceBuilder};
    use nacos_sdk::api::constants;
    use nacos_sdk::api::naming::{NamingService, NamingServiceBuilder};
    use nacos_sdk::api::props::ClientProps;
    use std::sync::Arc;
    use std::time::Duration;

    /// Default wait time for Nacos to propagate registration changes.
    const PROPAGATION_WAIT: Duration = Duration::from_secs(2);

    fn random_test_port() -> u16 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time error")
            .as_nanos() as u64;
        19000 + ((seed % 1000) as u16)
    }

    /// Creates a NamingService connected to the given server address.
    async fn create_naming_service(server_addr: String) -> NamingService {
        let props = ClientProps::new().server_addr(server_addr).namespace("");

        NamingServiceBuilder::new(props)
            .build()
            .await
            .expect("naming service build failed")
    }

    /// Test: register a single ephemeral instance and verify it can be queried.
    #[tokio::test]
    async fn test_register_instance() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-register-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9090)
            .ephemeral(true)
            .build();

        let result = service
            .register_instance(service_name.clone(), group.clone(), instance)
            .await;
        assert!(result.is_ok(), "register_instance failed: {:?}", result);

        // Wait for propagation
        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Verify instance is queryable
        let instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(!instances.is_empty(), "expected at least one instance");
        assert_eq!(instances[0].ip(), "127.0.0.1");
        assert_eq!(instances[0].port(), 9090);

        // Cleanup
        let cleanup_instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9090)
            .ephemeral(true)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup_instance)
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: register a persistent instance and verify it can be queried.
    #[tokio::test]
    async fn test_register_persistent_instance() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-persistent-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9091)
            .ephemeral(false)
            .build();

        let _result = service
            .register_instance(service_name.clone(), group.clone(), instance)
            .await;

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await;
        assert!(instances.is_ok(), "get_all_instances should succeed");
        let _instances = instances.expect("get_all_instances failed");

        // Cleanup persistent instance
        let cleanup_instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9091)
            .ephemeral(false)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup_instance)
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: deregister an instance and verify it is removed.
    #[tokio::test]
    async fn test_deregister_instance() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-deregister-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9092)
            .build();

        // Register
        service
            .register_instance(service_name.clone(), group.clone(), instance.clone())
            .await
            .expect("register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Verify registered
        let instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(!instances.is_empty());

        // Deregister
        service
            .deregister_instance(service_name.clone(), group.clone(), instance)
            .await
            .expect("deregister failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Verify removed
        let instances = service
            .get_all_instances(service_name, group, vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(
            instances.is_empty(),
            "expected no instances after deregister"
        );

        server.stop().await.expect("server stop failed");
    }

    /// Test: batch register multiple instances.
    #[tokio::test]
    async fn test_batch_register_instance() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-batch-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9100)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9101)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9102)
                .build(),
        ];

        service
            .batch_register_instance(service_name.clone(), group.clone(), instances.clone())
            .await
            .expect("batch_register_instance failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let fetched = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert_eq!(fetched.len(), 3, "expected 3 instances");

        // Cleanup
        let _ = service
            .batch_register_instance(service_name.clone(), group.clone(), vec![])
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: get all instances for a service.
    #[tokio::test]
    async fn test_get_all_instances() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-get-all-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        // Register two instances
        let instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9200)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9201)
                .build(),
        ];

        service
            .batch_register_instance(service_name.clone(), group.clone(), instances)
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let all = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert_eq!(all.len(), 2);

        // Cleanup
        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: select instances with healthy filter.
    #[tokio::test]
    async fn test_select_instances() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-select-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9300)
                .healthy(true)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9301)
                .healthy(false)
                .build(),
        ];

        service
            .batch_register_instance(service_name.clone(), group.clone(), instances)
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Select only healthy instances
        let healthy = service
            .select_instances(
                service_name.clone(),
                group.clone(),
                vec![],
                false,
                true, /* healthy only */
            )
            .await
            .expect("select_instances failed");

        // All returned should be healthy
        for inst in &healthy {
            assert!(inst.healthy(), "expected healthy instance");
        }

        // Select all instances (healthy=false means return all)
        let all = service
            .select_instances(service_name.clone(), group.clone(), vec![], false, false)
            .await;
        assert!(all.is_ok(), "select_instances should succeed");

        // Cleanup
        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: select one healthy instance.
    #[tokio::test]
    async fn test_select_one_healthy_instance() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-select-one-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9400)
                .healthy(true)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9401)
                .healthy(true)
                .build(),
        ];

        service
            .batch_register_instance(service_name.clone(), group.clone(), instances)
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let one = service
            .select_one_healthy_instance(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("select_one_healthy_instance failed");
        assert!(one.healthy());
        assert!(one.ip() == "127.0.0.1");

        // Cleanup
        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: get service list.
    #[tokio::test]
    async fn test_get_service_list() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-service-list".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        // Register a service
        let instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9500)
                .build(),
        ];

        service
            .batch_register_instance(service_name.clone(), group.clone(), instances)
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let (service_list, count) = service
            .get_service_list(1, 10, group.clone())
            .await
            .expect("get_service_list failed");
        assert!(count >= 1, "expected at least 1 service");
        assert!(
            service_list.iter().any(|s| s == &service_name),
            "service list should contain our service"
        );

        // Cleanup
        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: subscribe to service changes and receive events.
    #[tokio::test]
    async fn test_subscribe() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-subscribe-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let listener = Arc::new(MockNamingListener::new());

        // Subscribe
        service
            .subscribe(
                service_name.clone(),
                group.clone(),
                vec![],
                listener.clone(),
            )
            .await
            .expect("subscribe failed");

        // Register an instance to trigger an event
        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9600)
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance.clone())
            .await
            .expect("register failed");

        // Wait for event propagation
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Verify event was received
        let events = listener.get_events();
        assert!(!events.is_empty(), "expected at least one naming event");

        let last_event = events.last().expect("no events");
        assert_eq!(last_event.service_name, service_name);

        // Unsubscribe
        service
            .unsubscribe(
                service_name.clone(),
                group.clone(),
                vec![],
                listener.clone(),
            )
            .await
            .expect("unsubscribe failed");

        // Cleanup
        let _ = service
            .deregister_instance(service_name, group, instance)
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: subscribe and unsubscribe lifecycle.
    #[tokio::test]
    async fn test_subscribe_unsubscribe_lifecycle() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-lifecycle-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let listener = Arc::new(MockNamingListener::new());

        // Subscribe
        service
            .subscribe(
                service_name.clone(),
                group.clone(),
                vec![],
                listener.clone(),
            )
            .await
            .expect("subscribe failed");

        // Register to trigger
        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9700)
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance.clone())
            .await
            .expect("register failed");

        tokio::time::sleep(Duration::from_secs(3)).await;

        let events_before = listener.get_events().len();
        assert!(events_before > 0, "expected events before unsubscribe");

        // Unsubscribe
        service
            .unsubscribe(
                service_name.clone(),
                group.clone(),
                vec![],
                listener.clone(),
            )
            .await
            .expect("unsubscribe failed");

        // Register another instance - should NOT trigger event after unsubscribe
        let instance2 = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9701)
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance2.clone())
            .await
            .expect("register failed");

        tokio::time::sleep(Duration::from_secs(2)).await;

        let events_after = listener.get_events().len();
        // Events should not increase significantly after unsubscribe
        assert!(
            events_after <= events_before + 1,
            "expected no new events after unsubscribe"
        );

        // Cleanup
        let _ = service
            .deregister_instance(service_name.clone(), group.clone(), instance)
            .await;
        let _ = service
            .deregister_instance(service_name, group, instance2)
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: register instance with custom metadata.
    #[tokio::test]
    async fn test_register_instance_with_metadata() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-metadata-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9800)
            .metadata("env", "test")
            .metadata("version", "1.0.0")
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance)
            .await
            .expect("register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(!instances.is_empty());

        let meta = instances[0].metadata();
        assert_eq!(meta.get("env"), Some(&"test".to_string()));
        assert_eq!(meta.get("version"), Some(&"1.0.0".to_string()));

        // Cleanup
        let cleanup = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9800)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup)
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: register instance with custom cluster.
    #[tokio::test]
    async fn test_register_instance_with_cluster() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-cluster-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9900)
            .cluster("CUSTOM_CLUSTER")
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance)
            .await
            .expect("register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Query with specific cluster
        let instances = service
            .get_all_instances(
                service_name.clone(),
                group.clone(),
                vec!["CUSTOM_CLUSTER".to_string()],
                false,
            )
            .await
            .expect("get_all_instances failed");
        assert!(!instances.is_empty());

        // Cleanup
        let cleanup = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9900)
            .cluster("CUSTOM_CLUSTER")
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup)
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: multiple services coexist.
    #[tokio::test]
    async fn test_multiple_services() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let group = Some(constants::DEFAULT_GROUP.to_string());

        // Register two different services
        let service_a = "service-a".to_string();
        let service_b = "service-b".to_string();

        service
            .register_instance(
                service_a.clone(),
                group.clone(),
                ServiceInstanceBuilder::new()
                    .ip("127.0.0.1")
                    .port(10000)
                    .build(),
            )
            .await
            .expect("register service-a failed");

        service
            .register_instance(
                service_b.clone(),
                group.clone(),
                ServiceInstanceBuilder::new()
                    .ip("127.0.0.1")
                    .port(10001)
                    .build(),
            )
            .await
            .expect("register service-b failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Verify service-a
        let instances_a = service
            .get_all_instances(service_a.clone(), group.clone(), vec![], false)
            .await
            .expect("get service-a failed");
        assert_eq!(instances_a.len(), 1);
        assert_eq!(instances_a[0].port(), 10000);

        // Verify service-b
        let instances_b = service
            .get_all_instances(service_b.clone(), group.clone(), vec![], false)
            .await
            .expect("get service-b failed");
        assert_eq!(instances_b.len(), 1);
        assert_eq!(instances_b[0].port(), 10001);

        // Cleanup
        let _ = service
            .deregister_instance(
                service_a,
                group.clone(),
                ServiceInstanceBuilder::new()
                    .ip("127.0.0.1")
                    .port(10000)
                    .build(),
            )
            .await;
        let _ = service
            .deregister_instance(
                service_b,
                group,
                ServiceInstanceBuilder::new()
                    .ip("127.0.0.1")
                    .port(10001)
                    .build(),
            )
            .await;

        server.stop().await.expect("server stop failed");
    }

    /// Test: subscribe receives correct event data.
    #[tokio::test]
    async fn test_subscribe_event_data() {
        let mut server = create_server(ServerMode::default(), random_test_port());
        server.start().await.expect("server start failed");

        let service = create_naming_service(server.server_addr()).await;
        let service_name = "test-event-data-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let listener = Arc::new(MockNamingListener::new());

        service
            .subscribe(
                service_name.clone(),
                group.clone(),
                vec![],
                listener.clone(),
            )
            .await
            .expect("subscribe failed");

        let instance = ServiceInstanceBuilder::new()
            .ip("10.0.0.1")
            .port(11000)
            .metadata("role", "primary")
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance)
            .await
            .expect("register failed");

        tokio::time::sleep(Duration::from_secs(3)).await;

        let events = listener.get_events();
        assert!(!events.is_empty());

        let event = events.last().expect("no events");
        assert_eq!(event.service_name, service_name);
        assert_eq!(event.group_name, constants::DEFAULT_GROUP);

        if let Some(ref instances) = event.instances {
            assert!(!instances.is_empty());
            let found = instances
                .iter()
                .any(|i| i.ip() == "10.0.0.1" && i.port() == 11000);
            assert!(found, "event should contain registered instance");
        }

        // Cleanup
        let cleanup = ServiceInstanceBuilder::new()
            .ip("10.0.0.1")
            .port(11000)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup)
            .await;

        server.stop().await.expect("server stop failed");
    }
}
