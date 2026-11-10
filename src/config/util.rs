/// A special splitter that reduces user-defined character repetition.
const GROUP_KEY_SPLIT: &str = "+_+";

/// group to data_id '+_+' group '+_+' namespace
pub(crate) fn group_key(data_id: &str, group: &str, namespace: &str) -> String {
    format!("{data_id}{GROUP_KEY_SPLIT}{group}{GROUP_KEY_SPLIT}{namespace}")
}

#[cfg(test)]
mod tests {
    use crate::config::util;

    #[test]
    fn test_group_key() {
        let data_id = "data_id";
        let group = "group";
        let namespace = "namespace";

        let group_key = util::group_key(data_id, group, namespace);
        assert_eq!(group_key, "data_id+_+group+_+namespace");
    }
}
