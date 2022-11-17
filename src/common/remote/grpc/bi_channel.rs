use std::sync::Arc;

use crate::{
    api::error::Error::ErrResult, api::error::Error::GrpcioJoin, api::error::Result,
    common::executor, nacos_proto::v2::Payload,
};
use futures::{SinkExt, StreamExt};
use grpcio::{ClientDuplexReceiver, StreamingCallSink, WriteFlags};
use tokio::sync::mpsc::{self, Sender};
use tracing::{error, warn};

pub(crate) struct BiChannel {
    local_message_sender: Sender<Result<Payload>>,
}

pub(crate) struct ResponseWriter {
    local_message_sender: Sender<Result<Payload>>,
}

impl Clone for ResponseWriter {
    fn clone(&self) -> Self {
        Self {
            local_message_sender: self.local_message_sender.clone(),
        }
    }
}

impl ResponseWriter {
    pub(crate) fn new(local_message_sender: Sender<Result<Payload>>) -> Self {
        Self {
            local_message_sender,
        }
    }

    pub(crate) async fn write(&self, payload: Payload) -> Result<()> {
        let ret = self.local_message_sender.send(Ok(payload)).await;
        if let Err(e) = ret {
            warn!("ResponseWriter write message occur an error. {:?}", e);
            return Err(ErrResult(
                "ResponseWriter write message occur an error".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn blocking_write(&self, payload: Payload) -> Result<()> {
        let ret = self.local_message_sender.blocking_send(Ok(payload));
        if let Err(e) = ret {
            warn!("ResponseWriter write message occur an error. {:?}", e);
            return Err(ErrResult(
                "ResponseWriter write message occur an error".to_string(),
            ));
        }

        Ok(())
    }
}

impl BiChannel {
    pub(crate) fn new<F>(
        bi_stream: (StreamingCallSink<Payload>, ClientDuplexReceiver<Payload>),
        processor: Arc<F>,
    ) -> Self
    where
        F: Fn(Payload, ResponseWriter) + Send + Sync + 'static,
    {
        let (local_message_sender, mut local_message_receiver) =
            mpsc::channel::<Result<Payload>>(1024);

        let (mut server_message_sender, mut server_message_receiver) = bi_stream;

        let local_message_sender_for_read_server_message_task = local_message_sender.clone();
        let read_server_message_task = async move {
            let local_message_sender = local_message_sender_for_read_server_message_task;
            while let Some(payload) = server_message_receiver.next().await {
                if let Err(e) = payload {
                    error!("receive message  occur an error: {:?}, close bi channel", e);
                    let _ = local_message_sender.send(Err(GrpcioJoin(e))).await;
                    break;
                }

                let payload = payload.unwrap();

                let response_writer = ResponseWriter::new(local_message_sender.clone());
                let processor = processor.clone();
                let processor_task = async move {
                    processor(payload, response_writer);
                };
                executor::spawn(processor_task);
            }

            error!("read server message task quit.");
            let _ = local_message_sender
                .send(Err(ErrResult("read server message quit".to_string())))
                .await;
        };

        let write_server_message_task = async move {
            while let Some(payload) = local_message_receiver.recv().await {
                if let Err(e) = payload {
                    error!("send message occur an error: {:?}, close bi channel", e);
                    local_message_receiver.close();
                    let _ = server_message_sender.close().await;
                    break;
                }

                let payload = payload.unwrap();
                let _ = server_message_sender
                    .send((payload, WriteFlags::default()))
                    .await;
            }

            error!("write server message task quit.");
            let _ = server_message_sender.close().await;
        };

        executor::spawn(read_server_message_task);
        executor::spawn(write_server_message_task);

        Self {
            local_message_sender,
        }
    }

    pub(crate) async fn write(&self, payload: Payload) -> Result<()> {
        let ret = self.local_message_sender.send(Ok(payload)).await;
        if let Err(e) = ret {
            warn!("send message occur an error. {:?}", e);
            return Err(ErrResult("send message occur an error".to_string()));
        }

        Ok(())
    }

    pub(crate) fn blocking_write(&self, payload: Payload) -> Result<()> {
        let ret = self.local_message_sender.blocking_send(Ok(payload));

        if let Err(e) = ret {
            warn!("send message occur an error. {:?}", e);
            return Err(ErrResult("send message occur an error".to_string()));
        }

        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<()> {
        let ret = self
            .local_message_sender
            .send(Err(ErrResult("close bi channel".to_string())))
            .await;
        if let Err(e) = ret {
            warn!("close channel occur an error. {:?}", e);
            return Err(ErrResult("close channel occur an error".to_string()));
        }
        Ok(())
    }

    pub(crate) fn blocking_close(&self) -> Result<()> {
        let ret = self
            .local_message_sender
            .blocking_send(Err(ErrResult("close bi channel".to_string())));
        if let Err(e) = ret {
            warn!("close channel occur an error. {:?}", e);
            return Err(ErrResult("close channel occur an error".to_string()));
        }
        Ok(())
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.local_message_sender.is_closed()
    }

    pub(crate) async fn closed(&self) {
        self.local_message_sender.closed().await
    }
}
