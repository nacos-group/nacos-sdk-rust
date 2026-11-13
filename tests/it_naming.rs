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
    use crate::fixtures::ServerMode;
    use crate::fixtures::shared_server::{get_server_mode, get_shared_server_addr};
    use crate::shared::test_data::{MockNamingListener, ServiceInstanceBuilder};
    use nacos_sdk::api::constants;
    use nacos_sdk::api::naming::{NamingService, NamingServiceBuilder};
    use nacos_sdk::api::props::ClientProps;
    use std::sync::Arc;
    use std::time::Duration;

    const PROPAGATION_WAIT: Duration = Duration::from_secs(2);

    async fn create_naming_service(server_addr: String) -> NamingService {
        let props = ClientProps::new().server_addr(server_addr).namespace("");

        NamingServiceBuilder::new(props)
            .build()
            .await
            .expect("naming service build failed")
    }

    #[tokio::test]
    async fn test_register_instance() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(!instances.is_empty(), "expected at least one instance");
        assert_eq!(instances[0].ip(), "127.0.0.1");
        assert_eq!(instances[0].port(), 9090);

        let cleanup_instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9090)
            .ephemeral(true)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup_instance)
            .await;
    }

    #[tokio::test]
    async fn test_register_persistent_instance() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        let cleanup_instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9091)
            .ephemeral(false)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup_instance)
            .await;
    }

    #[tokio::test]
    async fn test_deregister_instance() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-deregister-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9092)
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance.clone())
            .await
            .expect("register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(!instances.is_empty());

        service
            .deregister_instance(service_name.clone(), group.clone(), instance)
            .await
            .expect("deregister failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let instances = service
            .get_all_instances(service_name, group, vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(
            instances.is_empty(),
            "expected no instances after deregister"
        );
    }

    #[tokio::test]
    async fn test_batch_register_instance() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        let _ = service
            .batch_register_instance(service_name.clone(), group.clone(), vec![])
            .await;
    }

    #[tokio::test]
    async fn test_get_service_list() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-service-list".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9300)
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance.clone())
            .await
            .expect("register_instance failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let (service_names, count) = service
            .get_service_list(1, 1000, group.clone())
            .await
            .expect("get_service_list failed");
        assert!(count >= 1, "expected at least one service");
        assert!(
            service_names
                .iter()
                .any(|name| name.contains(&service_name)),
            "registered service missing from list: {service_names:?}"
        );

        let _ = service
            .deregister_instance(service_name, group, instance)
            .await;
    }

    #[tokio::test]
    async fn test_get_all_instances() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-get-all-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

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

        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    /// Test: a fetched instance list is an immutable snapshot.
    ///
    /// Instances registered by another client must not mutate a list already returned.
    #[tokio::test]
    async fn test_instance_list_is_immutable_snapshot() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr.clone()).await;
        let service_name = "test-snapshot-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let first_instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9500)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9501)
                .build(),
        ];
        service
            .batch_register_instance(service_name.clone(), group.clone(), first_instances)
            .await
            .expect("batch_register failed");
        tokio::time::sleep(PROPAGATION_WAIT).await;

        let first = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert_eq!(first.len(), 2);

        let other_instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.2")
            .port(9502)
            .build();
        let other_service = create_naming_service(server_addr).await;
        other_service
            .register_instance(service_name.clone(), group.clone(), other_instance.clone())
            .await
            .expect("register failed");
        tokio::time::sleep(PROPAGATION_WAIT).await;

        let second = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert_eq!(second.len(), 3);
        assert_eq!(
            first.len(),
            2,
            "previously returned snapshot must not change"
        );
        assert!(
            first.iter().any(|i| i.port() == 9500) && first.iter().any(|i| i.port() == 9501),
            "snapshot should keep the original instances"
        );

        let _ = other_service
            .deregister_instance(service_name.clone(), group.clone(), other_instance)
            .await;
        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    /// Test: cache hits share the same instance list.
    ///
    /// Two reads of an unchanged subscription must return the same `Arc`.
    #[tokio::test]
    async fn test_instance_list_cache_hits_share_the_list() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-shared-instances-service".to_string();
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

        let instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9510)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9511)
                .build(),
        ];
        service
            .batch_register_instance(service_name.clone(), group.clone(), instances)
            .await
            .expect("batch_register failed");

        for _ in 0..50 {
            if !listener.get_events().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            !listener.get_events().is_empty(),
            "expected a push to populate the cache"
        );

        let first = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], true)
            .await
            .expect("get_all_instances failed");
        let second = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], true)
            .await
            .expect("get_all_instances failed");

        assert_eq!(first.len(), 2);
        assert!(
            Arc::ptr_eq(&first, &second),
            "cache hits must share the same instance list"
        );

        service
            .unsubscribe(service_name.clone(), group.clone(), vec![], listener)
            .await
            .expect("unsubscribe failed");
        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    /// Test: empty service boundaries.
    ///
    /// A missing service yields an empty list and cannot select an instance.
    #[tokio::test]
    async fn test_empty_service_boundaries() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-empty-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let all = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances should succeed for an empty service");
        assert!(all.is_empty());

        let one = service
            .select_one_healthy_instance(service_name, group, vec![], false)
            .await;
        assert!(
            one.is_err(),
            "selecting from an empty service must return an error"
        );
    }

    #[tokio::test]
    async fn test_select_instances() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        let healthy = service
            .select_instances(service_name.clone(), group.clone(), vec![], false, true)
            .await
            .expect("select_instances failed");

        for inst in healthy.iter() {
            assert!(inst.healthy(), "expected healthy instance");
        }

        let all = service
            .select_instances(service_name.clone(), group.clone(), vec![], false, false)
            .await;
        assert!(all.is_ok(), "select_instances should succeed");

        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    #[tokio::test]
    async fn test_select_one_healthy_instance() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    #[tokio::test]
    async fn test_subscribe() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-subscribe-service".to_string();
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
            .ip("127.0.0.1")
            .port(9600)
            .build();

        service
            .register_instance(service_name.clone(), group.clone(), instance.clone())
            .await
            .expect("register failed");

        tokio::time::sleep(Duration::from_secs(3)).await;

        let events = listener.get_events();
        assert!(!events.is_empty(), "expected at least one naming event");

        let last_event = events.last().expect("no events");
        assert_eq!(last_event.service_name, service_name);

        service
            .unsubscribe(
                service_name.clone(),
                group.clone(),
                vec![],
                listener.clone(),
            )
            .await
            .expect("unsubscribe failed");

        let _ = service
            .deregister_instance(service_name, group, instance)
            .await;
    }

    #[tokio::test]
    async fn test_subscribe_unsubscribe_lifecycle() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-lifecycle-service".to_string();
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

        service
            .unsubscribe(
                service_name.clone(),
                group.clone(),
                vec![],
                listener.clone(),
            )
            .await
            .expect("unsubscribe failed");

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
        assert!(
            events_after <= events_before + 1,
            "expected no new events after unsubscribe"
        );

        let _ = service
            .deregister_instance(service_name.clone(), group.clone(), instance)
            .await;
        let _ = service
            .deregister_instance(service_name, group, instance2)
            .await;
    }

    #[tokio::test]
    async fn test_register_instance_with_metadata() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        let cleanup = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9800)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup)
            .await;
    }

    #[tokio::test]
    async fn test_register_instance_with_cluster() {
        if get_server_mode() == ServerMode::ExternallyManaged {
            return;
        }

        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        let cleanup = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9900)
            .cluster("CUSTOM_CLUSTER")
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup)
            .await;
    }

    /// Test: the cluster filter is applied client-side, like the Java client.
    ///
    /// The Java client subscribes without a cluster filter so the cached
    /// service info holds every instance, then narrows the result down in
    /// memory. This registers instances in two clusters and checks that a
    /// cluster-scoped query returns only that cluster while an unscoped query
    /// returns everything.
    #[tokio::test]
    async fn test_get_all_instances_filters_by_cluster() {
        if get_server_mode() == ServerMode::ExternallyManaged {
            return;
        }

        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-multi-cluster-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let cluster_a = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9910)
            .cluster("CLUSTER_A")
            .build();
        let cluster_b = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9911)
            .cluster("CLUSTER_B")
            .build();

        service
            .batch_register_instance(
                service_name.clone(),
                group.clone(),
                vec![cluster_a.clone(), cluster_b.clone()],
            )
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // A cluster-scoped query returns only that cluster.
        let scoped = service
            .get_all_instances(
                service_name.clone(),
                group.clone(),
                vec!["CLUSTER_A".to_string()],
                false,
            )
            .await
            .expect("get_all_instances failed");
        assert!(
            scoped
                .iter()
                .all(|instance| { instance.cluster_name.as_deref() == Some("CLUSTER_A") }),
            "a cluster-scoped query must only return instances of that cluster"
        );
        assert!(
            scoped
                .iter()
                .any(|instance| instance.cluster_name.as_deref() == Some("CLUSTER_A")),
            "the requested cluster's instance should be returned"
        );

        // An unscoped query returns every instance.
        let all = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(
            all.iter()
                .any(|instance| instance.cluster_name.as_deref() == Some("CLUSTER_A"))
                && all
                    .iter()
                    .any(|instance| instance.cluster_name.as_deref() == Some("CLUSTER_B")),
            "an unscoped query must return the instances of every cluster"
        );

        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    /// Test: a cached result is only served while the service is subscribed.
    ///
    /// The Java client checks that the service is still subscribed before it
    /// serves a cached service info, and re-subscribes otherwise — a cached
    /// entry can be left over from a subscription that was dropped, or come
    /// from the on-disk cache at startup.
    #[tokio::test]
    async fn test_get_all_instances_resubscribes_after_unsubscribe() {
        if get_server_mode() == ServerMode::ExternallyManaged {
            return;
        }

        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-resubscribe-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let listener = Arc::new(MockNamingListener::default());
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
            .ip("127.0.0.1")
            .port(9920)
            .build();

        service
            .batch_register_instance(service_name.clone(), group.clone(), vec![instance.clone()])
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // The push populated the cache, so this is served from it.
        let first = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], true)
            .await
            .expect("get_all_instances failed");
        assert_eq!(first.len(), 1);

        // Drop the subscription. The cached entry stays behind, but the
        // service is no longer subscribed.
        service
            .unsubscribe(service_name.clone(), group.clone(), vec![], listener)
            .await
            .expect("unsubscribe failed");

        // Deregistering the instance makes the cached entry stale. A query
        // must not keep serving it: the client has to re-subscribe and pick up
        // the current state.
        service
            .deregister_instance(service_name.clone(), group.clone(), instance)
            .await
            .expect("deregister failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let second = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], true)
            .await
            .expect("get_all_instances failed");
        assert!(
            second.is_empty(),
            "a stale cached result must not be served after the subscription was dropped"
        );
    }

    /// Test: unsubscribing one listener keeps the others subscribed.
    ///
    /// The server-side subscription and the polling task are shared by all
    /// the listeners of a service, so they may only be dropped once the last
    /// listener is gone — the Java client checks `isSubscribed` first.
    #[tokio::test]
    async fn test_unsubscribe_keeps_other_listeners_alive() {
        if get_server_mode() == ServerMode::ExternallyManaged {
            return;
        }

        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-two-listeners-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let first = Arc::new(MockNamingListener::default());
        let second = Arc::new(MockNamingListener::default());

        service
            .subscribe(service_name.clone(), group.clone(), vec![], first.clone())
            .await
            .expect("subscribe failed");
        service
            .subscribe(service_name.clone(), group.clone(), vec![], second.clone())
            .await
            .expect("subscribe failed");

        service
            .batch_register_instance(
                service_name.clone(),
                group.clone(),
                vec![
                    ServiceInstanceBuilder::new()
                        .ip("127.0.0.1")
                        .port(9930)
                        .build(),
                ],
            )
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Drop the first listener. The second one must stay subscribed.
        service
            .unsubscribe(service_name.clone(), group.clone(), vec![], first)
            .await
            .expect("unsubscribe failed");

        service
            .batch_register_instance(
                service_name.clone(),
                group.clone(),
                vec![
                    ServiceInstanceBuilder::new()
                        .ip("127.0.0.1")
                        .port(9931)
                        .build(),
                ],
            )
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        assert!(
            second.get_events().len() >= 2,
            "the remaining listener must keep receiving pushes after the other one unsubscribed"
        );

        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    /// Test: unsubscribing an unregistered listener is a no-op.
    ///
    /// The server-side subscription is shared by all the listeners of a
    /// service, so it may only be dropped once the last listener is gone.
    /// Unsubscribing a listener that was never registered must not drop it —
    /// the Java client treats removing an unknown listener as a no-op.
    #[tokio::test]
    async fn test_unsubscribe_unknown_listener_keeps_subscription() {
        if get_server_mode() == ServerMode::ExternallyManaged {
            return;
        }

        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-unknown-listener-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let kept = Arc::new(MockNamingListener::default());
        let unknown = Arc::new(MockNamingListener::default());

        service
            .subscribe(service_name.clone(), group.clone(), vec![], kept.clone())
            .await
            .expect("subscribe failed");

        // Unsubscribing a listener that was never registered must not affect
        // the subscription the other listener relies on.
        service
            .unsubscribe(service_name.clone(), group.clone(), vec![], unknown)
            .await
            .expect("unsubscribe failed");

        service
            .batch_register_instance(
                service_name.clone(),
                group.clone(),
                vec![
                    ServiceInstanceBuilder::new()
                        .ip("127.0.0.1")
                        .port(9940)
                        .build(),
                ],
            )
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        assert!(
            !kept.get_events().is_empty(),
            "the registered listener must keep receiving pushes after an unrelated unsubscribe"
        );

        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    /// Test: unsubscribing an already-removed listener keeps the others alive.
    ///
    /// The second unsubscribe of the same listener finds nothing to remove.
    /// That must be a no-op — the shared server-side subscription still
    /// belongs to the listeners that are left.
    #[tokio::test]
    async fn test_unsubscribe_twice_keeps_other_listeners_alive() {
        if get_server_mode() == ServerMode::ExternallyManaged {
            return;
        }

        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let service_name = "test-double-unsubscribe-service".to_string();
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let first = Arc::new(MockNamingListener::default());
        let second = Arc::new(MockNamingListener::default());

        service
            .subscribe(service_name.clone(), group.clone(), vec![], first.clone())
            .await
            .expect("subscribe failed");
        service
            .subscribe(service_name.clone(), group.clone(), vec![], second.clone())
            .await
            .expect("subscribe failed");

        service
            .batch_register_instance(
                service_name.clone(),
                group.clone(),
                vec![
                    ServiceInstanceBuilder::new()
                        .ip("127.0.0.1")
                        .port(9941)
                        .build(),
                ],
            )
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        // Drop the first listener.
        service
            .unsubscribe(service_name.clone(), group.clone(), vec![], first.clone())
            .await
            .expect("unsubscribe failed");

        // Unsubscribing it again must not affect the second listener.
        service
            .unsubscribe(service_name.clone(), group.clone(), vec![], first)
            .await
            .expect("unsubscribe failed");

        service
            .batch_register_instance(
                service_name.clone(),
                group.clone(),
                vec![
                    ServiceInstanceBuilder::new()
                        .ip("127.0.0.1")
                        .port(9942)
                        .build(),
                ],
            )
            .await
            .expect("batch_register failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        assert!(
            second.get_events().len() >= 2,
            "the remaining listener must keep receiving pushes after a duplicate unsubscribe, \
             got {} events",
            second.get_events().len()
        );

        let _ = service
            .batch_register_instance(service_name, group, vec![])
            .await;
    }

    #[tokio::test]
    async fn test_multiple_services() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
        let group = Some(constants::DEFAULT_GROUP.to_string());

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

        let instances_a = service
            .get_all_instances(service_a.clone(), group.clone(), vec![], false)
            .await
            .expect("get service-a failed");
        assert_eq!(instances_a.len(), 1);
        assert_eq!(instances_a[0].port(), 10000);

        let instances_b = service
            .get_all_instances(service_b.clone(), group.clone(), vec![], false)
            .await
            .expect("get service-b failed");
        assert_eq!(instances_b.len(), 1);
        assert_eq!(instances_b[0].port(), 10001);

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
    }

    #[tokio::test]
    async fn test_subscribe_event_data() {
        let server_addr = get_shared_server_addr().await;
        let service = create_naming_service(server_addr).await;
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

        let cleanup = ServiceInstanceBuilder::new()
            .ip("10.0.0.1")
            .port(11000)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup)
            .await;
    }
}
