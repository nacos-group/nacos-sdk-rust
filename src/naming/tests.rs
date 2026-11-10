use core::time;
use std::collections::HashMap;

use tracing::info;

use crate::api::{naming::NamingChangeEvent, plugin::NoopAuthPlugin};

use super::*;

fn tracing_log_try_init() {
    crate::test_config::setup_log();
}

#[test]
fn test_is_selectable_boundaries() {
    let mut instance = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        ..Default::default()
    };
    assert!(is_selectable(&instance, true));
    assert!(!is_selectable(&instance, false));

    instance.healthy = false;
    assert!(!is_selectable(&instance, true));
    assert!(is_selectable(&instance, false));

    instance.healthy = true;
    instance.enabled = false;
    assert!(!is_selectable(&instance, true));

    instance.enabled = true;
    instance.weight = 0.0;
    assert!(!is_selectable(&instance, true));

    instance.weight = -1.0;
    assert!(!is_selectable(&instance, true));

    instance.weight = f64::NAN;
    assert!(!is_selectable(&instance, true));

    instance.weight = f64::INFINITY;
    assert!(is_selectable(&instance, true));
}

#[tokio::test]
#[ignore]
async fn test_ephemeral_register_service() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata,
        ..Default::default()
    };

    let ret = naming_service
        .register_instance("test-ephemeral-service".to_string(), None, service_instance)
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(1);
    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_persistent_register_service() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 8848,
        ephemeral: false,
        metadata,
        ..Default::default()
    };

    let ret = naming_service
        .register_instance(
            "test-persistent-service".to_string(),
            None,
            service_instance,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(1);
    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_register_and_deregister_persistent_service() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 8848,
        metadata,
        ephemeral: false,
        ..Default::default()
    };

    let ret = naming_service
        .register_instance(
            "test-persistent-service".to_string(),
            None,
            service_instance.clone(),
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(3);
    tokio::time::sleep(ten_millis).await;

    let ret = naming_service
        .deregister_instance(
            "test-persistent-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            service_instance,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(1);
    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_register_and_deregister_ephemeral_service() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata,
        ..Default::default()
    };

    let ret = naming_service
        .register_instance(
            "test-ephemeral-service".to_string(),
            None,
            service_instance.clone(),
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(3);
    tokio::time::sleep(ten_millis).await;

    let ret = naming_service
        .deregister_instance(
            "test-ephemeral-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            service_instance,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(1);
    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_batch_register_service() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance2 = ServiceInstance {
        ip: "192.168.1.1".to_string(),
        port: 8888,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance3 = ServiceInstance {
        ip: "172.0.2.1".to_string(),
        port: 6666,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let instance_vec = vec![service_instance1, service_instance2, service_instance3];

    let ret = naming_service
        .batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(3);
    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_batch_register_service_and_query_all_instances() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance2 = ServiceInstance {
        ip: "192.168.1.1".to_string(),
        port: 8888,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance3 = ServiceInstance {
        ip: "172.0.2.1".to_string(),
        port: 6666,
        metadata: metadata.clone(),
        ..Default::default()
    };
    let instance_vec = vec![service_instance1, service_instance2, service_instance3];

    let ret = naming_service
        .batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(3);
    tokio::time::sleep(ten_millis).await;

    let all_instances = naming_service
        .get_all_instances(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            false,
        )
        .await;
    info!("response. {all_instances:?}");

    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_select_instance() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance2 = ServiceInstance {
        ip: "192.168.1.1".to_string(),
        port: 8888,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance3 = ServiceInstance {
        ip: "172.0.2.1".to_string(),
        port: 6666,
        metadata: metadata.clone(),
        ..Default::default()
    };
    let instance_vec = vec![service_instance1, service_instance2, service_instance3];

    let ret = naming_service
        .batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(3);
    tokio::time::sleep(ten_millis).await;

    let all_instances = naming_service
        .select_instances(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            false,
            true,
        )
        .await;
    info!("response. {all_instances:?}");

    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_select_one_healthy_instance() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance2 = ServiceInstance {
        ip: "192.168.1.1".to_string(),
        port: 8888,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance3 = ServiceInstance {
        ip: "172.0.2.1".to_string(),
        port: 6666,
        metadata: metadata.clone(),
        ..Default::default()
    };
    let instance_vec = vec![service_instance1, service_instance2, service_instance3];

    let ret = naming_service
        .batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(3);
    tokio::time::sleep(ten_millis).await;

    for _ in 0..3 {
        let all_instances = naming_service
            .select_one_healthy_instance(
                "test-service".to_string(),
                Some(crate::api::constants::DEFAULT_GROUP.to_string()),
                Vec::default(),
                false,
            )
            .await;
        info!("response. {all_instances:?}");
    }

    tokio::time::sleep(ten_millis).await;
}

#[tokio::test]
#[ignore]
async fn test_get_service_list() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance2 = ServiceInstance {
        ip: "192.168.1.1".to_string(),
        port: 8888,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance3 = ServiceInstance {
        ip: "172.0.2.1".to_string(),
        port: 6666,
        metadata: metadata.clone(),
        ..Default::default()
    };
    let instance_vec = vec![service_instance1, service_instance2, service_instance3];

    let ret = naming_service
        .batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        )
        .await;
    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(3);
    tokio::time::sleep(ten_millis).await;

    let service_list = naming_service.get_service_list(1, 50, None).await;
    info!("response. {service_list:?}");

    tokio::time::sleep(ten_millis).await;
}

#[derive(Hash, PartialEq)]
pub struct InstancesChangeEventListener;

impl NamingEventListener for InstancesChangeEventListener {
    fn event(&self, event: Arc<NamingChangeEvent>) {
        info!("InstancesChangeEventListener: {event:?}");
    }
}

#[tokio::test]
#[ignore]
async fn test_service_push() {
    tracing_log_try_init();

    let props = ClientProps::new().server_addr("127.0.0.1:8848");

    let mut metadata = HashMap::<String, String>::new();
    metadata.insert("netType".to_string(), "external".to_string());
    metadata.insert("version".to_string(), "2.0".to_string());

    let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))
        .await
        .expect("Failed to create NacosNamingService in test");
    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance2 = ServiceInstance {
        ip: "192.168.1.1".to_string(),
        port: 8888,
        metadata: metadata.clone(),
        ..Default::default()
    };

    let service_instance3 = ServiceInstance {
        ip: "172.0.2.1".to_string(),
        port: 6666,
        metadata: metadata.clone(),
        ..Default::default()
    };
    let instance_vec = vec![service_instance1, service_instance2, service_instance3];

    let ret = naming_service
        .batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        )
        .await;
    info!("response. {ret:?}");

    let listener = Arc::new(InstancesChangeEventListener);
    let ret = naming_service
        .subscribe(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            listener,
        )
        .await;

    info!("response. {ret:?}");

    let ten_millis = time::Duration::from_secs(1);
    tokio::time::sleep(ten_millis).await;
}
