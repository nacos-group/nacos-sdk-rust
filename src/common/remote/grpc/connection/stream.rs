use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

use crate::api::error::Error;
use crate::common::remote::grpc::handlers::default_handler::DefaultHandler;
use crate::common::remote::grpc::message::request::SetupAckRequest;
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageData};
use crate::common::remote::grpc::nacos_grpc_service::GrpcStream;
use crate::common::util::now_millis;
use crate::nacos_proto::v2::Payload;

use super::{ConnectionEvent, HandlerMap};

/// Outcome of the setup-ack wait. `Ok(None)` means the ack arrived without an
/// ability table; `Err` carries why no usable ack was seen (the stream ended
/// first, or the ack payload could not be parsed).
pub(crate) type SetupAck = Result<Option<HashMap<String, bool>>, String>;

pub(crate) async fn run(
    mut server_stream: GrpcStream<Result<Payload, Error>>,
    local_tx: mpsc::Sender<Payload>,
    handler_map: Arc<HandlerMap>,
    setup_ack_tx: oneshot::Sender<SetupAck>,
    events_tx: mpsc::Sender<ConnectionEvent>,
    last_active: Arc<AtomicU64>,
) {
    let mut setup_ack_tx = Some(setup_ack_tx);
    let reason = loop {
        let Some(item) = server_stream.next().await else {
            break Error::ErrResult("server stream closed".to_string());
        };
        match item {
            Err(error) => break error,
            Ok(payload) => {
                last_active.store(now_millis(), Ordering::Release);
                let Some(metadata) = payload.metadata.as_ref() else {
                    debug!("server payload has no metadata, skip");
                    continue;
                };
                if metadata.r#type == SetupAckRequest::identity().as_ref() {
                    match GrpcMessage::<SetupAckRequest>::from_payload(payload) {
                        Ok(message) => {
                            if let Some(tx) = setup_ack_tx.take() {
                                let _ = tx.send(Ok(message.into_body().ability_table));
                            }
                        }
                        // A malformed ack must fail negotiation instead of being
                        // mistaken for an empty ability table.
                        Err(error) => {
                            if let Some(tx) = setup_ack_tx.take() {
                                let _ = tx.send(Err(format!("invalid SetupAckRequest: {error}")));
                            }
                        }
                    }
                    continue;
                }
                let handler = handler_map
                    .get(&metadata.r#type)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(DefaultHandler));
                if let Some(reply) = handler.request_reply(payload).await
                    && local_tx.send(reply).await.is_err()
                {
                    break Error::ErrResult("bi stream outbound channel is closed".to_string());
                }
            }
        }
    };
    // Hand the closure cause to `negotiate` through the still-pending ack sender
    // before reporting the terminal event; otherwise the oneshot simply resolves
    // on drop and the caller only sees "channel closed", losing `reason`.
    if let Some(tx) = setup_ack_tx.take() {
        let _ = tx.send(Err(format!("bi stream closed before setup ack: {reason}")));
    }
    let _ = events_tx.send(ConnectionEvent::StreamClosed(reason)).await;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use tokio::sync::{mpsc, oneshot};

    use super::*;
    use crate::common::remote::grpc::message::GrpcMessageBuilder;
    use crate::common::remote::grpc::message::request::SetupAckRequest;
    use crate::common::remote::grpc::nacos_grpc_service::ServerRequestHandler;

    struct CountingHandler(Arc<AtomicUsize>);

    #[async_trait]
    impl ServerRequestHandler for CountingHandler {
        // Echoes the request back so a misrouted ack would visibly produce an
        // outbound reply, making the `local_rx` emptiness check meaningful.
        async fn request_reply(&self, request: Payload) -> Option<Payload> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Some(request)
        }
    }

    #[tokio::test]
    async fn test_setup_ack_is_intercepted_without_handler_or_reply() {
        let ack = SetupAckRequest {
            ability_table: Some(HashMap::new()),
            ..Default::default()
        };
        let ack_payload = GrpcMessageBuilder::new(ack)
            .build()
            .into_payload()
            .expect("ack payload");
        let server_stream = GrpcStream::new(Box::pin(futures::stream::iter(vec![Ok(ack_payload)])));
        let (local_tx, mut local_rx) = mpsc::channel(8);
        let (ack_tx, ack_rx) = oneshot::channel();
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut handlers: HandlerMap = HashMap::new();
        handlers.insert(
            "SetupAckRequest".to_string(),
            Arc::new(CountingHandler(calls.clone())) as Arc<dyn ServerRequestHandler>,
        );
        run(
            server_stream,
            local_tx,
            Arc::new(handlers),
            ack_tx,
            events_tx,
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
        )
        .await;
        assert!(matches!(ack_rx.await.expect("ack"), Ok(Some(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(local_rx.recv().await.is_none());
        assert!(events_rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn test_null_ability_table_is_accepted_as_none() {
        let ack_payload = GrpcMessageBuilder::new(SetupAckRequest {
            ability_table: None,
            ..Default::default()
        })
        .build()
        .into_payload()
        .expect("ack payload");
        let server_stream = GrpcStream::new(Box::pin(futures::stream::iter(vec![Ok(ack_payload)])));
        let (local_tx, _local_rx) = mpsc::channel(8);
        let (ack_tx, ack_rx) = oneshot::channel();
        let (events_tx, _events_rx) = mpsc::channel(8);
        run(
            server_stream,
            local_tx,
            Arc::new(HandlerMap::new()),
            ack_tx,
            events_tx,
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
        )
        .await;
        assert_eq!(ack_rx.await.expect("ack"), Ok(None));
    }

    #[tokio::test]
    async fn test_malformed_setup_ack_fails_negotiation() {
        let mut ack_payload = GrpcMessageBuilder::new(SetupAckRequest {
            ability_table: Some(HashMap::new()),
            ..Default::default()
        })
        .build()
        .into_payload()
        .expect("ack payload");
        ack_payload.body.as_mut().expect("ack body").value = b"not a setup ack".to_vec();
        let server_stream = GrpcStream::new(Box::pin(futures::stream::iter(vec![Ok(ack_payload)])));
        let (local_tx, _local_rx) = mpsc::channel(8);
        let (ack_tx, ack_rx) = oneshot::channel();
        let (events_tx, _events_rx) = mpsc::channel(8);
        run(
            server_stream,
            local_tx,
            Arc::new(HandlerMap::new()),
            ack_tx,
            events_tx,
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
        )
        .await;
        let result = ack_rx.await.expect("ack");
        let message = result.expect_err("a malformed ack must fail negotiation");
        assert!(
            message.contains("invalid SetupAckRequest"),
            "unexpected message: {message}"
        );
    }
}
