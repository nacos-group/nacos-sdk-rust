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
    // shape so a refactor of the request definitions cannot silently change it.
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
    assert_eq!(
        json.get("type").and_then(|v| v.as_str()),
        Some("registerInstance")
    );
    assert_eq!(
        json.get("instance")
            .and_then(|v| v.get("ip"))
            .and_then(|v| v.as_str()),
        Some("127.0.0.1")
    );
    assert_eq!(json.get("namespace").and_then(|v| v.as_str()), Some("ns"));
    assert_eq!(
        json.get("serviceName").and_then(|v| v.as_str()),
        Some("svc")
    );
    assert_eq!(json.get("groupName").and_then(|v| v.as_str()), Some("grp"));
    assert!(json.get("requestId").is_some());

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
    assert!(json.get("requestId").is_some());
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

    instance.weight = f64::NAN;
    assert!(!is_selectable(&instance, true));

    instance.weight = f64::INFINITY;
    assert!(is_selectable(&instance, true));
}
