//! Common internal components for the Nacos SDK.
//!
//! This module contains shared utilities and infrastructure used by both
//! the config and naming modules:
//!
//! - `cache`: Disk-based caching for persistent storage
//! - `error`: Error handling utilities and response processing
//! - `executor`: Async task scheduling and thread pool management
//! - `remote`: gRPC client implementation for Nacos server communication
//! - `util`: Common utility functions (client ID generation, validation)
//! - `log`: Tracing/log initialization (optional, enabled via `tracing-log` feature)

pub(crate) mod cache;
pub(crate) mod error;
pub(crate) mod executor;
pub(crate) mod remote;
pub(crate) mod util;

#[cfg(feature = "tracing-log")]
pub(crate) mod log;
