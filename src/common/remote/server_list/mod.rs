mod provider;
pub(crate) use provider::*;

mod service;
pub(crate) use service::*;

pub(crate) trait ServerAddress: Sync + Send + 'static {
    fn host(&self) -> String;

    fn port(&self) -> u32;

    fn is_available(&self) -> bool;
}

use crate::api::constants::{DEFAULT_SERVER_PORT, ENV_NACOS_CLIENT_COMMON_SERVER_PORT};
use crate::api::error::Error;
use crate::api::error::Result;
use crate::properties::get_value_u32;

/// Parse a server address string (comma/semicolon/newline separated) into a list of `host:port`.
///
/// Each segment without an explicit port gets the default port appended.
pub(crate) fn parse_server_list(server_addr: &str) -> Result<Vec<String>> {
    let hosts = server_addr
        .trim()
        .split([',', ';', '\n'])
        .map(str::trim)
        .filter(|host| !host.is_empty());

    let de_port = get_value_u32(ENV_NACOS_CLIENT_COMMON_SERVER_PORT, DEFAULT_SERVER_PORT);
    let mut result = vec![];
    for host in hosts {
        let host_port = host.split(':').collect::<Vec<&str>>();
        if host_port.len() == 1 {
            result.push(format!("{}:{}", host, de_port));
            continue;
        }
        result.push(host.to_string());
    }

    if result.is_empty() {
        return Err(Error::WrongServerAddress("Server address is empty".into()));
    }

    Ok(result)
}

/// Parse a list of `host:port` strings into `(host, port)` tuples.
///
/// Returns `Err` if the list is empty or all entries have illegal format.
pub(crate) fn parse_host_port(server_list: &[String]) -> Result<Vec<(String, u32)>> {
    if server_list.is_empty() {
        return Err(Error::WrongServerAddress(
            "server list must not empty".into(),
        ));
    }

    let result_list: Vec<(String, u32)> = server_list
        .iter()
        .filter_map(|server| {
            let mut parts = server.split(':');
            let address = parts.next()?;
            let port = parts.next()?;
            let port = port.parse::<u32>().ok()?;
            Some((address.to_string(), port))
        })
        .collect();
    if result_list.is_empty() {
        return Err(Error::WrongServerAddress(
            "all the server is illegal format!".into(),
        ));
    }
    Ok(result_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── parse_server_list ───

    #[test]
    fn test_parse_server_list_comma_separated() {
        let result = parse_server_list("127.0.0.1,192.168.0.1").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8848", "192.168.0.1:8848"]);
    }

    #[test]
    fn test_parse_server_list_semicolon_separated() {
        let result = parse_server_list("127.0.0.1;192.168.0.1").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8848", "192.168.0.1:8848"]);
    }

    #[test]
    fn test_parse_server_list_newline_separated() {
        let result = parse_server_list("127.0.0.1\n192.168.0.1").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8848", "192.168.0.1:8848"]);
    }

    #[test]
    fn test_parse_server_list_mixed_separators() {
        let result =
            parse_server_list("127.0.0.1,192.168.0.1;10.0.0.1\n172.16.0.1").expect("should parse");
        assert_eq!(
            result,
            vec![
                "127.0.0.1:8848",
                "192.168.0.1:8848",
                "10.0.0.1:8848",
                "172.16.0.1:8848"
            ]
        );
    }

    #[test]
    fn test_parse_server_list_default_port_appended() {
        let result = parse_server_list("127.0.0.1").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8848"]);
    }

    #[test]
    fn test_parse_server_list_explicit_port_preserved() {
        let result = parse_server_list("127.0.0.1:8850").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8850"]);
    }

    #[test]
    fn test_parse_server_list_mixed_default_and_explicit_port() {
        let result = parse_server_list("127.0.0.1;192.168.0.1:8850").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8848", "192.168.0.1:8850"]);
    }

    #[test]
    fn test_parse_server_list_whitespace_trimmed() {
        let result = parse_server_list("  127.0.0.1 ,  192.168.0.1  ").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8848", "192.168.0.1:8848"]);
    }

    #[test]
    fn test_parse_server_list_empty_input() {
        let result = parse_server_list("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_server_list_whitespace_only() {
        let result = parse_server_list("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_server_list_trailing_separator() {
        let result = parse_server_list("127.0.0.1,").expect("should parse");
        assert_eq!(result, vec!["127.0.0.1:8848"]);
    }

    #[test]
    fn test_parse_server_list_single_ipv6_style_not_split() {
        // Colons beyond the first port colon are kept as-is — the split only checks len == 1 vs != 1
        let result = parse_server_list("[::1]:8848").expect("should parse");
        assert_eq!(result, vec!["[::1]:8848"]);
    }

    // ─── parse_host_port ───

    #[test]
    fn test_parse_host_port_single() {
        let result = parse_host_port(&["127.0.0.1:8848".to_string()]).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1".to_string(), 8848)]);
    }

    #[test]
    fn test_parse_host_port_multiple() {
        let result =
            parse_host_port(&["127.0.0.1:8848".to_string(), "192.168.0.1:8850".to_string()])
                .expect("should parse");
        assert_eq!(
            result,
            vec![
                ("127.0.0.1".to_string(), 8848),
                ("192.168.0.1".to_string(), 8850),
            ]
        );
    }

    #[test]
    fn test_parse_host_port_empty_input() {
        let result = parse_host_port(&Vec::<String>::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_no_port() {
        let result = parse_host_port(&["127.0.0.1".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_non_numeric_port() {
        let result = parse_host_port(&["127.0.0.1:abc".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_filters_illegal_keeps_valid() {
        let result = parse_host_port(&[
            "127.0.0.1:8848".to_string(),
            "bad_host".to_string(),
            "192.168.0.1:8850".to_string(),
        ])
        .expect("should parse");
        assert_eq!(
            result,
            vec![
                ("127.0.0.1".to_string(), 8848),
                ("192.168.0.1".to_string(), 8850),
            ]
        );
    }

    #[test]
    fn test_parse_host_port_port_zero() {
        let result = parse_host_port(&["127.0.0.1:0".to_string()]).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1".to_string(), 0)]);
    }

    #[test]
    fn test_parse_host_port_large_port() {
        let result = parse_host_port(&["127.0.0.1:65535".to_string()]).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1".to_string(), 65535)]);
    }

    #[test]
    fn test_parse_host_port_port_above_u16_still_parses_as_u32() {
        let result = parse_host_port(&["127.0.0.1:99999".to_string()]).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1".to_string(), 99999)]);
    }

    #[test]
    fn test_parse_host_port_negative_port() {
        let result = parse_host_port(&["127.0.0.1:-1".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_roundtrip_with_parse_server_list() {
        let parsed = parse_server_list("127.0.0.1,192.168.0.1:8850").expect("should parse");
        let host_ports = parse_host_port(&parsed).expect("should parse");
        assert_eq!(
            host_ports,
            vec![
                ("127.0.0.1".to_string(), 8848),
                ("192.168.0.1".to_string(), 8850),
            ]
        );
    }
}
