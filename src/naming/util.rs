use crate::api::constants::DEFAULT_GROUP;

/// Returns the group name or the default group if empty.
pub(crate) fn normalize_group_name(group_name: Option<String>) -> String {
    group_name
        .filter(|data| !data.is_empty())
        .unwrap_or_else(|| DEFAULT_GROUP.to_owned())
}

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

#[cfg(test)]
mod tests {
    use crate::naming::util;

    #[test]
    fn test_normalize_group_name() {
        assert_eq!(
            util::normalize_group_name(None),
            crate::api::constants::DEFAULT_GROUP
        );
        assert_eq!(
            util::normalize_group_name(Some(String::new())),
            crate::api::constants::DEFAULT_GROUP
        );
        assert_eq!(util::normalize_group_name(Some("G".to_string())), "G");
    }

    #[test]
    fn test_unique_cluster_string_sorts_and_deduplicates() {
        let clusters = vec![
            "c2".to_string(),
            "c1".to_string(),
            "c2".to_string(),
            "c10".to_string(),
        ];
        assert_eq!(util::unique_cluster_string(&clusters), "c1,c10,c2");
        assert_eq!(util::unique_cluster_string(&[]), "");
    }
}
