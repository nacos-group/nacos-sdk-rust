pub mod grpc;

use crate::api::error::Error::WrongServerAddress;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::api::error::{Error, Result};

// odd by client request id.
const SEQUENCE_INITIAL_VALUE: i64 = 1;
const SEQUENCE_DELTA: i64 = 2;
static ATOMIC_SEQUENCE: AtomicI64 = AtomicI64::new(SEQUENCE_INITIAL_VALUE);

pub(crate) fn generate_request_id() -> String {
    let seq = ATOMIC_SEQUENCE.fetch_add(SEQUENCE_DELTA, Ordering::Relaxed);
    if seq > i64::MAX - 1000 {
        ATOMIC_SEQUENCE.store(SEQUENCE_INITIAL_VALUE, Ordering::SeqCst);
    }
    seq.to_string()
}

/// make address's port plus 1000
pub(crate) fn into_grpc_server_addr(address: &str) -> Result<String> {
    let hosts = address.split(',').collect::<Vec<&str>>();
    if hosts.len() == 0 {
        return Err(WrongServerAddress(address.into()));
    }

    let mut result = vec![];
    for host in hosts {
        let host_port_pair = host.split(':').collect::<Vec<&str>>();
        if host_port_pair.len() != 2 {
            return Err(WrongServerAddress(address.into()));
        }

        let host = host_port_pair.get(0);
        let port = host_port_pair.get(1);
        if host.is_none() || port.is_none() {
            return Err(WrongServerAddress(address.into()));
        }

        let port = port
            .unwrap()
            .parse::<u32>()
            .map(|port| port + 1000)
            .map_err(|_| WrongServerAddress(address.into()))?;

        result.push(format!("{}:{}", host.unwrap(), port));
    }

    match result.len() {
        0 => Err(WrongServerAddress(address.into())),
        1 => Ok(format!("{}", result.get(0).unwrap())),
        _ => Ok(format!("ipv4:{}", result.join(","))),
    }
}

#[cfg(test)]
mod tests {
    use crate::common::remote::into_grpc_server_addr;

    #[test]
    fn test_empty_address() {
        match into_grpc_server_addr("") {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn test_host_address_without_port() {
        match into_grpc_server_addr("127.0.0.1") {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn test_host_addresses_without_one_port() {
        match into_grpc_server_addr("127.0.0.1:8848,127.0.0.1") {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn test_single_host_address() {
        let addr = "127.0.0.1:8848";
        let expected = "127.0.0.1:9848";
        let result = into_grpc_server_addr(addr).unwrap();
        assert_eq!(expected, result);
    }

    #[test]
    fn test_multiple_ipv4_address() {
        let addr = "127.0.0.1:8848,127.0.0.1:8849,127.0.0.1:8850";
        let expected = "ipv4:127.0.0.1:9848,127.0.0.1:9849,127.0.0.1:9850";
        let result = into_grpc_server_addr(addr).unwrap();
        assert_eq!(expected, result);
    }
}
