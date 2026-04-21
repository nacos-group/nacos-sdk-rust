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

/// Checks param_val not blank
pub(crate) fn check_not_blank<'a>(param_val: &'a str, param_name: &'a str) -> Result<&'a str> {
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
    fn test_check_not_blank_fail() {
        let data_id = "";
        assert!(check_not_blank(data_id, "data_id").is_err());

        let data_id = "   ";
        assert!(check_not_blank(data_id, "data_id").is_err());
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
