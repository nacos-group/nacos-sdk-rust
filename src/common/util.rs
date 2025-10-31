use crate::api::error::Error;
use crate::api::error::Result;

pub(crate) static LOCAL_IP: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| local_ipaddress::get().unwrap_or(String::from("127.0.0.1")));

pub(crate) static HOME_DIR: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    home::home_dir()
        .unwrap_or(std::env::temp_dir())
        .as_path()
        .to_str()
        .unwrap()
        .to_owned()
});

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
    use crate::common::util::check_not_blank;

    #[test]
    fn test_check_not_blank() {
        let data_id = "data_id";
        let group = "group";
        let namespace = "namespace";

        assert_eq!(data_id, check_not_blank(data_id, "data_id").unwrap());
        assert_eq!(group, check_not_blank(group, "group").unwrap());
        assert_eq!(namespace, check_not_blank(namespace, "namespace").unwrap());
    }

    #[test]
    fn test_check_not_blank_fail() {
        let data_id = "";
        assert!(check_not_blank(data_id, "data_id").is_err());

        let data_id = "   ";
        assert!(check_not_blank(data_id, "data_id").is_err());
    }
}
