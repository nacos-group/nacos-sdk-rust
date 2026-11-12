use crate::api::error::Error;
use crate::api::error::Result;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) static LOCAL_IP: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| local_ipaddress::get().unwrap_or(String::from("127.0.0.1")));

#[allow(dead_code)]
pub(crate) static HOME_DIR: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::home_dir()
        .unwrap_or(std::env::temp_dir())
        .to_str()
        .map(|s| s.to_owned())
        .unwrap_or_else(|| "/tmp".to_owned())
});

/// Generates unique client ID for the given module.
/// Format: `{module_name}-_-{server_addr}-_-{namespace}-_-{sequence}`
pub(crate) fn generate_client_id(
    module_name: &str,
    server_addr: &str,
    namespace: &str,
    seq: &AtomicU64,
) -> String {
    format!(
        "{module_name}-_-{server_addr}-_-{namespace}-_-{}",
        seq.fetch_add(1, Ordering::SeqCst)
    )
}

/// Returns the group name or default group if empty.
pub(crate) fn normalize_group_name(group_name: Option<String>) -> String {
    group_name
        .filter(|data| !data.is_empty())
        .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned())
}

/// Checks whether two slices contain the same elements regardless of order.
/// Duplicates are significant: `[a, a]` != `[a]`.
pub(crate) fn unordered_eq<T>(a: &[T], b: &[T]) -> bool
where
    T: Ord,
{
    if a.len() != b.len() {
        return false;
    }
    let mut a_sorted: Vec<&T> = a.iter().collect();
    let mut b_sorted: Vec<&T> = b.iter().collect();
    a_sorted.sort();
    b_sorted.sort();
    a_sorted == b_sorted
}

/// The separator the Nacos client puts between the group and the service name.
pub(crate) const GROUPED_NAME_SEPARATOR: &str = "@@";

/// Joins cluster names into the canonical form used by the Nacos client.
///
/// Sorted and deduplicated, matching the Java client's
/// `NamingSelectorFactory.getUniqueClusterString`.
pub(crate) fn unique_cluster_string(clusters: &[String]) -> String {
    let mut sorted: Vec<&str> = clusters.iter().map(|cluster| cluster.as_str()).collect();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.join(",")
}

/// Builds the `group@@service` name.
///
/// Matches the Java client's `NamingUtils.getGroupedName`. A service name
/// that already contains the separator is returned unchanged, and an empty
/// group leaves the service name alone.
pub(crate) fn grouped_service_name(service_name: &str, group_name: &str) -> String {
    if !group_name.is_empty() && !service_name.contains(GROUPED_NAME_SEPARATOR) {
        return format!("{group_name}{GROUPED_NAME_SEPARATOR}{service_name}");
    }
    service_name.to_string()
}

/// Writes `content`, truncated to `max` characters with a trailing `...`.
///
/// Used by the `Display` implementations of the config types, which show
/// only a prefix of the content.
pub(crate) fn write_truncated(
    f: &mut std::fmt::Formatter<'_>,
    content: &str,
    max: usize,
) -> std::fmt::Result {
    if content.chars().count() > max {
        for c in content.chars().take(max) {
            write!(f, "{c}")?;
        }
        write!(f, "...")
    } else {
        write!(f, "{content}")
    }
}

/// Checks that a parameter is not blank and returns it as an owned `String`.
///
/// Taking `impl Into<String>` folds the conversion and the check into one
/// step, so the API entry points do not repeat `let x = x.into();` next to
/// every check.
pub(crate) fn check_not_blank(param_val: impl Into<String>, param_name: &str) -> Result<String> {
    let param_val = param_val.into();
    if param_val.trim().is_empty() {
        Err(Error::InvalidParam(
            param_name.into(),
            "param must not blank!".into(),
        ))
    } else {
        Ok(param_val)
    }
}

#[cfg(test)]
mod tests {
    use crate::common::util::{check_not_blank, unordered_eq};

    #[test]
    fn test_unordered_eq_same_order() {
        assert!(unordered_eq(&["a", "b"], &["a", "b"]));
    }

    #[test]
    fn test_unordered_eq_different_order() {
        assert!(unordered_eq(&["b", "a"], &["a", "b"]));
    }

    #[test]
    fn test_unordered_eq_different_length() {
        assert!(!unordered_eq(&["a"], &["a", "b"]));
    }

    #[test]
    fn test_unordered_eq_duplicates_significant() {
        assert!(!unordered_eq(&["a", "a"], &["a"]));
    }

    #[test]
    fn test_unordered_eq_empty() {
        assert!(unordered_eq::<String>(&[], &[]));
    }

    #[test]
    fn test_check_not_blank() {
        let data_id = "data_id";
        let group = "group";
        let namespace = "namespace";

        assert_eq!(
            data_id,
            check_not_blank(data_id, "data_id")
                .expect("check_not_blank should return the original string")
        );
        assert_eq!(
            group,
            check_not_blank(group, "group")
                .expect("check_not_blank should return the original string")
        );
        assert_eq!(
            namespace,
            check_not_blank(namespace, "namespace")
                .expect("check_not_blank should return the original string")
        );
    }

    #[test]
    fn test_check_not_blank_accepts_owned_and_borrowed() {
        // The parameter is taken as `impl Into<String>`, so both a `&str` and
        // an owned `String` work.
        assert_eq!(
            "value",
            check_not_blank("value", "param").expect("should accept a &str")
        );
        assert_eq!(
            "value",
            check_not_blank("value".to_string(), "param").expect("should accept a String")
        );
    }

    #[test]
    fn test_check_not_blank_fail() {
        let data_id = "";
        assert!(check_not_blank(data_id, "data_id").is_err());

        let data_id = "   ";
        assert!(check_not_blank(data_id, "data_id").is_err());
    }

    #[test]
    fn test_grouped_service_name() {
        // A group is prepended with the separator.
        assert_eq!(
            super::grouped_service_name("svc", "DEFAULT"),
            "DEFAULT@@svc"
        );
        // A service name that already carries the separator is left alone.
        assert_eq!(super::grouped_service_name("a@@b", "DEFAULT"), "a@@b");
        // An empty group leaves the service name alone.
        assert_eq!(super::grouped_service_name("svc", ""), "svc");
    }

    #[test]
    fn test_normalize_group_name() {
        assert_eq!(
            super::normalize_group_name(None),
            crate::api::constants::DEFAULT_GROUP
        );
        assert_eq!(
            super::normalize_group_name(Some(String::new())),
            crate::api::constants::DEFAULT_GROUP
        );
        assert_eq!(super::normalize_group_name(Some("G".to_string())), "G");
    }

    #[test]
    fn test_generate_client_id() {
        use std::sync::atomic::AtomicU64;

        let seq = AtomicU64::new(1);
        let client_id = super::generate_client_id("config", "127.0.0.1:8848", "public", &seq);
        assert_eq!(client_id, "config-_-127.0.0.1:8848-_-public-_-1");

        let client_id2 = super::generate_client_id("config", "127.0.0.1:8848", "public", &seq);
        assert_eq!(client_id2, "config-_-127.0.0.1:8848-_-public-_-2");

        let seq2 = AtomicU64::new(1);
        let client_id3 = super::generate_client_id("naming", "127.0.0.1:8848", "public", &seq2);
        assert_eq!(client_id3, "naming-_-127.0.0.1:8848-_-public-_-1");

        let seq3 = AtomicU64::new(1);
        let client_id4 = super::generate_client_id(
            "config",
            "http://endpoint.example.com:8080/nacos/serverlist",
            "public",
            &seq3,
        );
        assert_eq!(
            client_id4,
            "config-_-http://endpoint.example.com:8080/nacos/serverlist-_-public-_-1"
        );
    }
}
