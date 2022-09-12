const GROUP_KEY_SPLIT: &str = "+";

/// group to data_id '+' group '+' tenant
pub(crate) fn group_key(data_id: &String, group: &String, tenant: &String) -> String {
    "".to_string()
        + data_id.as_str()
        + GROUP_KEY_SPLIT
        + group.as_str()
        + GROUP_KEY_SPLIT
        + tenant.as_str()
}

/// parse group_key to (data_id, group, tenant)
pub(crate) fn parse_key(group_key: &String) -> (String, String, String) {
    let v: Vec<&str> = group_key.split(GROUP_KEY_SPLIT).collect();
    (
        v.get(0).unwrap().to_string(),
        v.get(1).unwrap().to_string(),
        v.get(2).unwrap().to_string(),
    )
}
