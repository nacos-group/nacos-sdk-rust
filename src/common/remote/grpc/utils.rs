//! gRPC utility macros and helper functions

use crate::api::error::Error;
use tokio::sync::oneshot;
use tracing::error;

use super::nacos_grpc_service::Callback;

/// Converts a result, logging and returning an error on failure.
pub(crate) fn convert<T>(result: Result<T, Error>, context: &str) -> Result<T, Error> {
    result.map_err(|e| {
        error!("{}: {}", context, e);
        Error::ErrResult(context.to_string())
    })
}

/// Receives a response from a oneshot channel, logging and returning an error on failure.
pub(crate) fn recv_response<T>(
    result: Result<T, oneshot::error::RecvError>,
    context: &str,
) -> Result<T, Error> {
    result.map_err(|e| {
        error!("{}: {}", context, e);
        Error::ErrResult(context.to_string())
    })
}

/// Creates a new gRPC callback with the given result type.
pub(crate) fn create_grpc_callback<T>() -> (Callback<T>, oneshot::Receiver<T>, want::Taker) {
    let (gv, tk) = want::new();
    let (tx, rx) = oneshot::channel::<T>();
    (Callback::new(gv, tx), rx, tk)
}

/// Unwraps an option, returning an error if None.
pub(crate) fn unwrap_option<T>(opt: Option<T>, context: &str) -> Result<T, Error> {
    opt.ok_or_else(|| {
        error!("{}", context);
        Error::ErrResult(context.to_string())
    })
}
