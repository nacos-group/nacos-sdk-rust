/// A special splitter that reduces user-defined character repetition.
const GROUP_KEY_SPLIT: &str = "+_+";

/// group to data_id '+_+' group '+_+' namespace
pub(crate) fn group_key(data_id: &str, group: &str, namespace: &str) -> String {
    "".to_string() + data_id + GROUP_KEY_SPLIT + group + GROUP_KEY_SPLIT + namespace
}

/// parse group_key to (data_id, group, namespace)
#[allow(dead_code, clippy::get_first)]
pub(crate) fn parse_key(group_key: &str) -> (String, String, String) {
    let v: Vec<&str> = group_key.split(GROUP_KEY_SPLIT).collect();
    (
        v.get(0).expect("data_id should exist").to_string(),
        v.get(1).expect("group should exist").to_string(),
        v.get(2).expect("namespace should exist").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use crate::config::util;

    #[test]
    fn test_group_key_and_parse() {
        let data_id = "data_id";
        let group = "group";
        let namespace = "namespace";

        let group_key = util::group_key(data_id, group, namespace);
        let (parse_data_id, parse_group, parse_namespace) = util::parse_key(group_key.as_str());

        assert_eq!(parse_data_id, data_id);
        assert_eq!(parse_group, group);
        assert_eq!(parse_namespace, namespace);
    }
}
