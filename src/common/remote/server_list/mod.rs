mod provider;
pub(crate) use provider::*;

mod service;
pub(crate) use service::*;

pub(crate) trait ServerAddress: Sync + Send + 'static {
    fn host(&self) -> String;

    fn port(&self) -> u16;

    fn is_available(&self) -> bool;
}

use crate::api::constants::{DEFAULT_SERVER_PORT, ENV_NACOS_CLIENT_COMMON_SERVER_PORT};
use crate::api::error::Error;
use crate::api::error::Result;
use crate::properties::get_value_u16;

/// Parse a server address string (comma/semicolon/newline separated) into a list of `host:port`.
///
/// Each segment without an explicit port gets the default port appended.
pub(crate) fn parse_server_list(server_addr: &str) -> Result<Vec<String>> {
    let default_port = get_value_u16(ENV_NACOS_CLIENT_COMMON_SERVER_PORT, DEFAULT_SERVER_PORT);

    let result: Vec<String> = server_addr
        .trim()
        .split([',', ';', '\n'])
        .map(str::trim)
        .filter(|addr| !addr.is_empty())
        .map(|addr| {
            let (host, port) = parse_host_port(addr);
            format!("{}:{}", host, port.unwrap_or(default_port))
        })
        .collect();

    if result.is_empty() {
        tracing::warn!("Server address is empty after parsing: {:?}", server_addr);
        return Err(Error::WrongServerAddress("Server address is empty".into()));
    }

    Ok(result)
}

/// Parse a list of `host:port` strings into `(host, port)` tuples.
///
/// Returns `Err` if the list is empty or all entries have illegal format.
pub(crate) fn parse_host_port_vec(server_list: &[String]) -> Result<Vec<(&str, u16)>> {
    if server_list.is_empty() {
        return Err(Error::WrongServerAddress(
            "server list must not empty".into(),
        ));
    }

    let result_list: Vec<(&str, u16)> = server_list
        .iter()
        .filter_map(|server| {
            let (host, port) = parse_host_port(server);
            Some((host, port?))
        })
        .collect();
    if result_list.is_empty() {
        tracing::warn!(
            "All server addresses have illegal format: {:?}",
            server_list
        );
        return Err(Error::WrongServerAddress(
            "all the server is illegal format!".into(),
        ));
    }
    Ok(result_list)
}

/// Parse a bare `host` or `host:port` string into `(host, Some(port))` or `(host, None)`.
///
/// e.g. `"endpoint.example.com"`     → `("endpoint.example.com", None)`
///      `"endpoint.example.com:9090"` → `("endpoint.example.com", Some(9090))`
///      `"[::1]:8848"`               → `("[::1]", Some(8848))`
///      `"[::1]"`                    → `("[::1]", None)`
pub(crate) fn parse_host_port(input: &str) -> (&str, Option<u16>) {
    if input.starts_with('[') {
        // IPv6 bracket notation: [host]:port or [host]
        if let Some(bracket_end) = input.find(']') {
            let host = &input[..=bracket_end];
            let rest = &input[bracket_end + 1..];
            let port = rest.strip_prefix(':').and_then(|p| p.parse::<u16>().ok());
            return (host, port);
        }
    }
    if let Some(colon_pos) = input.find(':') {
        let (host, port_str) = input.split_at(colon_pos);
        if host.is_empty() {
            // Bare IPv6 without brackets (e.g. "::1") — treat entire input as host
            return (input, None);
        }
        let port = port_str[1..].parse::<u16>().ok();
        (host, port)
    } else {
        (input, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── parse_host_port ───

    #[test]
    fn test_parse_host_port_bare_hostname() {
        assert_eq!(
            parse_host_port("endpoint.example.com"),
            ("endpoint.example.com", None)
        );
    }

    #[test]
    fn test_parse_host_port_hostname_with_port() {
        assert_eq!(
            parse_host_port("endpoint.example.com:9090"),
            ("endpoint.example.com", Some(9090))
        );
    }

    #[test]
    fn test_parse_host_port_ipv4_no_port() {
        assert_eq!(parse_host_port("127.0.0.1"), ("127.0.0.1", None));
    }

    #[test]
    fn test_parse_host_port_ipv4_with_port() {
        assert_eq!(parse_host_port("127.0.0.1:8848"), ("127.0.0.1", Some(8848)));
    }

    #[test]
    fn test_parse_host_port_ipv6_with_port() {
        assert_eq!(parse_host_port("[::1]:8848"), ("[::1]", Some(8848)));
    }

    #[test]
    fn test_parse_host_port_ipv6_without_port() {
        assert_eq!(parse_host_port("[::1]"), ("[::1]", None));
    }

    #[test]
    fn test_parse_host_port_invalid_port_returns_none() {
        assert_eq!(parse_host_port("host:abc"), ("host", None));
    }

    #[test]
    fn test_parse_host_port_bare_ipv6_treated_as_host() {
        assert_eq!(parse_host_port("::1"), ("::1", None));
    }

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
    fn test_parse_server_list_ipv6_with_port() {
        let result = parse_server_list("[::1]:8848").expect("should parse");
        assert_eq!(result, vec!["[::1]:8848"]);
    }

    #[test]
    fn test_parse_server_list_ipv6_without_port() {
        let result = parse_server_list("[::1]").expect("should parse");
        assert_eq!(result, vec!["[::1]:8848"]);
    }

    // ─── parse_host_port_vec ───

    #[test]
    fn test_parse_host_port_vec_single() {
        let input = ["127.0.0.1:8848".to_string()];
        let result = parse_host_port_vec(&input).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1", 8848)]);
    }

    #[test]
    fn test_parse_host_port_vec_multiple() {
        let input = ["127.0.0.1:8848".to_string(), "192.168.0.1:8850".to_string()];
        let result = parse_host_port_vec(&input).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1", 8848), ("192.168.0.1", 8850)]);
    }

    #[test]
    fn test_parse_host_port_vec_empty_input() {
        let input: Vec<String> = vec![];
        let result = parse_host_port_vec(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_vec_no_port() {
        let input = ["127.0.0.1".to_string()];
        let result = parse_host_port_vec(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_vec_non_numeric_port() {
        let input = ["127.0.0.1:abc".to_string()];
        let result = parse_host_port_vec(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_vec_filters_illegal_keeps_valid() {
        let input = [
            "127.0.0.1:8848".to_string(),
            "bad_host".to_string(),
            "192.168.0.1:8850".to_string(),
        ];
        let result = parse_host_port_vec(&input).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1", 8848), ("192.168.0.1", 8850)]);
    }

    #[test]
    fn test_parse_host_port_vec_port_zero() {
        let input = ["127.0.0.1:0".to_string()];
        let result = parse_host_port_vec(&input).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1", 0)]);
    }

    #[test]
    fn test_parse_host_port_vec_large_port() {
        let input = ["127.0.0.1:65535".to_string()];
        let result = parse_host_port_vec(&input).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1", 65535)]);
    }

    #[test]
    fn test_parse_host_port_vec_port_above_u16_filtered_out() {
        let input = [
            "127.0.0.1:8848".to_string(),
            "192.168.0.1:99999".to_string(),
        ];
        let result = parse_host_port_vec(&input).expect("should parse");
        assert_eq!(result, vec![("127.0.0.1", 8848)]);
    }

    #[test]
    fn test_parse_host_port_vec_negative_port() {
        let input = ["127.0.0.1:-1".to_string()];
        let result = parse_host_port_vec(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_host_port_vec_ipv6() {
        let input = ["[::1]:8848".to_string()];
        let result = parse_host_port_vec(&input).expect("should parse");
        assert_eq!(result, vec![("[::1]", 8848)]);
    }

    #[test]
    fn test_parse_host_port_vec_roundtrip_with_parse_server_list() {
        let parsed = parse_server_list("127.0.0.1,192.168.0.1:8850").expect("should parse");
        let host_ports = parse_host_port_vec(&parsed).expect("should parse");
        assert_eq!(host_ports, vec![("127.0.0.1", 8848), ("192.168.0.1", 8850)]);
    }
}
