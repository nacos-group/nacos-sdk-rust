pub mod grpc;

use crate::api::error::Error::WrongGrpcAddress;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::api::error::Result;

const IPV4: &str = "ipv4";

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
    match address.split_once(':') {
        None => Err(WrongGrpcAddress(address.into())),
        Some((schema, addresses)) if schema.starts_with(IPV4) => {
            let host_port_pairs = addresses
                .split(',')
                .map(|host_port| {
                    let split = host_port.split(':').collect::<Vec<&str>>();
                    let host = split.get(0).unwrap();
                    let port = split.get(1).unwrap().parse::<u32>().unwrap() + 1000;
                    format!("{host}:{port}")
                })
                .collect::<Vec<String>>()
                .join(",");

            Ok(format!("{}:{}", schema, host_port_pairs))
        }
        Some((host, port)) => {
            let port = port.parse::<u32>().unwrap() + 1000;
            Ok(format!("{}:{}", host, port))
        }
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
    fn test_single_host_address() {
        let addr = "127.0.0.1:8848";
        let expected = "127.0.0.1:9848";
        let result = into_grpc_server_addr(addr).unwrap();
        assert_eq!(expected, result);
    }

    #[test]
    fn test_single_ipv4_address() {
        let addr = "ipv4:127.0.0.1:8848";
        let expected = "ipv4:127.0.0.1:9848";
        let result = into_grpc_server_addr(addr).unwrap();
        assert_eq!(expected, result);
    }

    #[test]
    fn test_multiple_ipv4_address() {
        let addr = "ipv4:127.0.0.1:8848,127.0.0.1:8849,127.0.0.1:8850";
        let expected = "ipv4:127.0.0.1:9848,127.0.0.1:9849,127.0.0.1:9850";
        let result = into_grpc_server_addr(addr).unwrap();
        assert_eq!(expected, result);
    }
}
