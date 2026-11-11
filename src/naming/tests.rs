use super::*;

#[test]
fn test_request_type_request_code() {
    // Naming `RequestType` through the glob re-export must be unambiguous.
    // Before the fix this name existed in two files, so referring to it here
    // failed to compile with E0659.
    assert_eq!(
        RequestType::Register.request_code(),
        "registerInstance".to_string()
    );
    assert_eq!(
        RequestType::Deregister.request_code(),
        "deregisterInstance".to_string()
    );
}

#[test]
fn test_instance_request_wire_format() {
    // The Nacos server dispatches on the serialized body. Pin the exact JSON
    // shape so a refactor of the request definitions cannot silently change
    // it. The whole object is compared, so an added, removed or renamed field
    // fails the test.
    let instance = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 8080,
        ..Default::default()
    };

    let ephemeral = InstanceRequest::register(
        instance.clone(),
        Some("svc".to_string()),
        Some("ns".to_string()),
        Some("grp".to_string()),
    );
    let json = serde_json::to_value(&ephemeral).expect("request should serialize");

    // `requestId` is generated per request, so it cannot be compared
    // literally; assert it is a non-empty string rather than merely present
    // (the key is always serialized, even when the value is null).
    let request_id = json
        .get("requestId")
        .and_then(|v| v.as_str())
        .expect("requestId should be a string");
    assert!(!request_id.is_empty(), "requestId should not be empty");

    // `requestId` is generated per request, so it is checked separately and
    // the rest of the object is compared exactly. Comparing the whole object
    // means an added, removed or renamed field fails this test.
    let mut object = json
        .as_object()
        .expect("request should serialize to an object")
        .clone();
    object.remove("requestId");

    // Parsed from a literal rather than built with `json!`: the macro expands
    // to `to_value(..).unwrap()` for non-boolean values, and `unwrap` is
    // disallowed by this crate's clippy configuration.
    let expected: serde_json::Value = serde_json::from_str(
        r#"{
            "type": "registerInstance",
            "namespace": "ns",
            "serviceName": "svc",
            "groupName": "grp",
            "headers": {},
            "instance": {
                "instanceId": null,
                "ip": "127.0.0.1",
                "port": 8080,
                "weight": 1.0,
                "healthy": true,
                "enabled": true,
                "ephemeral": true,
                "clusterName": "DEFAULT",
                "serviceName": null,
                "metadata": {}
            }
        }"#,
    )
    .expect("expected JSON should parse");
    assert_eq!(serde_json::Value::Object(object), expected);

    let persistent = PersistentInstanceRequest::deregister(
        instance,
        Some("svc".to_string()),
        Some("ns".to_string()),
        Some("grp".to_string()),
    );
    let json = serde_json::to_value(&persistent).expect("request should serialize");
    assert_eq!(
        json.get("type").and_then(|v| v.as_str()),
        Some("deregisterInstance")
    );
    assert_eq!(json.get("namespace").and_then(|v| v.as_str()), Some("ns"));
    assert!(
        json.get("requestId")
            .and_then(|v| v.as_str())
            .map(|id| !id.is_empty())
            .unwrap_or(false),
        "requestId should be a non-empty string"
    );
}

#[test]
fn test_instance_request_identity() {
    // The identity is what the server routes on; it must stay distinct.
    use crate::common::remote::grpc::message::GrpcMessageData;
    assert_ne!(
        InstanceRequest::identity(),
        PersistentInstanceRequest::identity()
    );
    assert_eq!(InstanceRequest::identity(), "InstanceRequest");
    assert_eq!(
        PersistentInstanceRequest::identity(),
        "PersistentInstanceRequest"
    );
}

#[test]
fn test_filter_instances_by_clusters() {
    // The Java client subscribes with an empty cluster list (Constants.NULL)
    // and filters the returned instances in memory, so the cache always holds
    // the full instance set. This mirrors NamingSelectorFactory's cluster
    // selector: an instance matches when its clusterName is in the requested
    // set.
    let instances = vec![
        ServiceInstance {
            ip: "10.0.0.1".to_string(),
            port: 8080,
            cluster_name: Some("c1".to_string()),
            ..Default::default()
        },
        ServiceInstance {
            ip: "10.0.0.2".to_string(),
            port: 8080,
            cluster_name: Some("c2".to_string()),
            ..Default::default()
        },
        ServiceInstance {
            ip: "10.0.0.3".to_string(),
            port: 8080,
            cluster_name: Some("DEFAULT".to_string()),
            ..Default::default()
        },
    ];

    // No cluster requested keeps every instance.
    let all = filter_instances_by_clusters(&instances, &[]);
    assert_eq!(all.len(), 3);

    // A single requested cluster keeps only that cluster.
    let c1 = filter_instances_by_clusters(&instances, &["c1".to_string()]);
    assert_eq!(c1.len(), 1);
    assert_eq!(c1[0].ip, "10.0.0.1");

    // Several requested clusters keep the union.
    let c1_c2 = filter_instances_by_clusters(&instances, &["c1".to_string(), "c2".to_string()]);
    assert_eq!(c1_c2.len(), 2);

    // An unknown cluster matches nothing.
    let unknown = filter_instances_by_clusters(&instances, &["nope".to_string()]);
    assert!(unknown.is_empty());
}

#[test]
fn test_filter_instances_by_clusters_without_cluster_name() {
    // The Java selector compares the raw cluster name, so an instance that
    // has none matches no cluster at all — not even the default one.
    let instances = vec![ServiceInstance {
        ip: "10.0.0.1".to_string(),
        port: 8080,
        cluster_name: None,
        ..Default::default()
    }];

    let default = filter_instances_by_clusters(
        &instances,
        &[crate::api::naming::DEFAULT_CLUSTER_NAME.to_string()],
    );
    assert!(default.is_empty());

    let other = filter_instances_by_clusters(&instances, &["c1".to_string()]);
    assert!(other.is_empty());

    // An unscoped query still returns it.
    let all = filter_instances_by_clusters(&instances, &[]);
    assert_eq!(all.len(), 1);
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
