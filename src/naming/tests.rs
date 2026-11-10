use super::*;

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
