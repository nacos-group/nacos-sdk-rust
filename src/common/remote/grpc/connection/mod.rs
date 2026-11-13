pub(crate) mod backoff;
pub(crate) mod config;

#[allow(unused_imports)] // re-exported for the connection supervisor
pub(crate) use config::ConnectionSettings;
