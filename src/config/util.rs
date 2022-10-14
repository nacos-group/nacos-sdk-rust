/// A special splitter that reduces user-defined character repetition.
const GROUP_KEY_SPLIT: &str = "+_+";

/// group to data_id '+_+' group '+_+' tenant
pub(crate) fn group_key(data_id: &str, group: &str, tenant: &str) -> String {
    "".to_string() + data_id + GROUP_KEY_SPLIT + group + GROUP_KEY_SPLIT + tenant
}

/// parse group_key to (data_id, group, tenant)
#[allow(clippy::get_first)]
pub(crate) fn parse_key(group_key: &str) -> (String, String, String) {
    let v: Vec<&str> = group_key.split(GROUP_KEY_SPLIT).collect();
    (
        v.get(0).unwrap().to_string(),
        v.get(1).unwrap().to_string(),
        v.get(2).unwrap().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use crate::config::util;

    #[test]
    fn test_group_key_and_parse() {
        let data_id = "data_id";
        let group = "group";
        let tenant = "tenant";

        let group_key = util::group_key(data_id, group, tenant);
        let (parse_data_id, parse_group, parse_tenant) = util::parse_key(group_key.as_str());

        assert_eq!(parse_data_id, data_id);
        assert_eq!(parse_group, group);
        assert_eq!(parse_tenant, tenant);
    }
}
