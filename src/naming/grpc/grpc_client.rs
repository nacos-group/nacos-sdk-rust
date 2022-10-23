use std::{
    sync::{
        atomic::{AtomicBool, AtomicI8, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::SinkExt;
use futures::TryStreamExt;
use grpcio::{
    CallOption, Channel, ChannelBuilder, ClientDuplexReceiver, ConnectivityState, Environment,
    LbPolicy, StreamingCallSink, WriteFlags,
};
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex, Notify,
};
use tracing::{debug, error, info, warn};

use crate::api::error::Result;
use crate::{api::error::Error::GrpcioJoin, nacos_proto::v2::Payload};
use crate::{
    api::{error::Error::ClientUnhealthy, events::common::GrpcReconnectedEvent},
    common::event_bus,
};
use crate::{
    common::executor,
    nacos_proto::v2::{BiRequestStreamClient, RequestClient},
};

use super::message::{GrpcMessage, GrpcMessageData};

pub struct GrpcClient {
    request_client: RequestClient,
    bi_request_stream_client: Arc<BiRequestStreamClient>,
    state: Arc<AtomicI8>,
    reconnect_notifier: Arc<Notify>,
    disconnect_notifier: Arc<Notify>,
}

impl GrpcClient {
    pub async fn new(address: &str) -> Self {
        info!("init grpc client: {}", address);
        let env = Arc::new(Environment::new(2));
        let channel = ChannelBuilder::new(env)
            .load_balancing_policy(LbPolicy::PickFirst)
            .connect(address);

        let deadline = Duration::from_secs(3);
        let is_connect = channel.wait_for_connected(deadline).await;
        if !is_connect {
            panic!("can't connect target server, please check network or the server address if it's wrong.")
        }

        let bi_channel = channel.clone();
        let request_client = RequestClient::new(channel.clone());
        let bi_request_stream_client = Arc::new(BiRequestStreamClient::new(bi_channel));

        let state = Arc::new(AtomicI8::new(GrpcClientState::Healthy.state_code()));

        let reconnect_notifier = Arc::new(Notify::new());

        let disconnect_notifier = Arc::new(Notify::new());

        Self::health_check(
            disconnect_notifier.clone(),
            reconnect_notifier.clone(),
            channel,
            state.clone(),
        );

        GrpcClient {
            request_client,
            bi_request_stream_client,
            state,
            reconnect_notifier,
            disconnect_notifier,
        }
    }

    async fn streaming_send_task(
        global_task_state: Arc<AtomicBool>,
        request_receiver: Arc<Mutex<Receiver<Payload>>>,
        mut response_sender: StreamingCallSink<Payload>,
    ) {
        let mut request_receiver = request_receiver.lock().await;

        while global_task_state.load(Ordering::Acquire) {
            let payload = request_receiver.recv().await;
            if payload.is_none() {
                continue;
            }
            let payload = payload.unwrap();

            let send_ret = response_sender.send((payload, WriteFlags::default())).await;
            if let Err(error) = send_ret {
                error!("send grpc message occur an error. {:?}", error);
            }
        }
    }

    async fn streaming_receive_task(
        global_task_state: Arc<AtomicBool>,
        mut request_receiver: ClientDuplexReceiver<Payload>,
        response_sender: Arc<Mutex<Sender<Payload>>>,
    ) {
        let response_sender = response_sender.lock().await;

        while global_task_state.load(Ordering::Acquire) {
            let payload = request_receiver.try_next().await;
            if let Err(e) = payload {
                error!("receive grpc message occur an error. {:?}", e);
                continue;
            }

            let payload = payload.unwrap();
            if payload.is_none() {
                info!("receive empty messages");
                continue;
            }

            let payload = payload.unwrap();

            let send_ret = response_sender.send(payload).await;
            if let Err(error) = send_ret {
                error!("send grpc message occur an error. {:?}", error);
            }
        }
    }

    async fn duplex_task(
        disconnect_notifier: Arc<Notify>,
        reconnect_notifier: Arc<Notify>,
        server_state: Arc<AtomicI8>,
        bi_request_stream_client: Arc<BiRequestStreamClient>,
        client_request_receiver: Receiver<Payload>,
        client_response_sender: Sender<Payload>,
    ) {
        let client_request_receiver = Arc::new(Mutex::new(client_request_receiver));
        let client_response_sender = Arc::new(Mutex::new(client_response_sender));

        loop {
            let current_state = server_state.load(Ordering::Acquire);
            let current_state = GrpcClientState::from(current_state);
            match current_state {
                GrpcClientState::Unhealthy => {
                    warn!("the current grpc client is in UNHEALTHY, duplex_task stop.");
                    reconnect_notifier.notified().await;
                    continue;
                }
                GrpcClientState::Shutdown => {
                    warn!("the current grpc client has already shutdown, duplex_task quit.");
                    break;
                }
                _ => {}
            }

            let call_opt = CallOption::default().wait_for_ready(true);
            let stream = bi_request_stream_client.request_bi_stream_opt(call_opt);
            if let Err(e) = stream {
                error!("request bi stream occur an error. {:?}", e);
                continue;
            }

            let (duplex_sink, duplex_receiver) = stream.unwrap();

            let global_task_state = Arc::new(AtomicBool::new(true));

            let send_task_handler = executor::spawn(Self::streaming_send_task(
                global_task_state.clone(),
                client_request_receiver.clone(),
                duplex_sink,
            ));
            let receive_task_handler = executor::spawn(Self::streaming_receive_task(
                global_task_state.clone(),
                duplex_receiver,
                client_response_sender.clone(),
            ));

            disconnect_notifier.notified().await;
            global_task_state.store(false, Ordering::Release);
            send_task_handler.abort();
            receive_task_handler.abort();
        }
    }

    pub async fn streaming_call(&self, request: Receiver<Payload>) -> Option<Receiver<Payload>> {
        let (rsp_sender, rsp_receiver) = channel::<Payload>(1024);

        let server_state = self.state.clone();
        let reconnect_notifier = self.reconnect_notifier.clone();
        let disconnect_notifier = self.disconnect_notifier.clone();
        let bi_request_stream_client = self.bi_request_stream_client.clone();

        executor::spawn(async move {
            Self::duplex_task(
                disconnect_notifier,
                reconnect_notifier,
                server_state,
                bi_request_stream_client,
                request,
                rsp_sender,
            )
            .await;
        });

        Some(rsp_receiver)
    }

    pub(crate) async fn unary_call_async<R, P>(
        &self,
        message: GrpcMessage<R>,
    ) -> Result<GrpcMessage<P>>
    where
        R: GrpcMessageData,
        P: GrpcMessageData,
    {
        let current_state = self.state.load(Ordering::Acquire);
        let current_state = GrpcClientState::from(current_state);
        match current_state {
            GrpcClientState::Unhealthy => {
                warn!("the current grpc client is in UNHEALTHY, it's not able to send message.");
                return Err(ClientUnhealthy(
                    "the current grpc client is in UNHEALTHY, it's not able to send message."
                        .to_string(),
                ));
            }
            GrpcClientState::Shutdown => {
                warn!("the current grpc client has already shutdown");
                return Err(ClientUnhealthy(
                    "the current grpc client has already shutdown.".to_string(),
                ));
            }
            _ => {}
        }

        let request_payload = message.into_payload();
        if request_payload.is_err() {
            let error = request_payload.unwrap_err();
            error!("unary_call_async error:{:?}", error);
            return Err(error);
        }
        let request_payload = request_payload.unwrap();

        let response_payload = self.request_client.request_async(&request_payload);

        if let Err(error) = response_payload {
            error!("receive grpc message occur an error. {:?}", error);
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap().await;

        if let Err(error) = response_payload {
            error!("receive grpc message occur an error. {:?}", error);
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap();

        let message = GrpcMessage::<P>::from_payload(response_payload);
        if let Err(error) = message {
            error!(
                "convert grpc payload to  message occur an error. {:?}",
                error
            );
            return Err(error);
        }
        Ok(message.unwrap())
    }

    fn health_check(
        disconnect_notifier: Arc<Notify>,
        reconnect_notifier: Arc<Notify>,
        channel: Channel,
        service_state: Arc<AtomicI8>,
    ) {
        let check_task = async move {
            loop {
                let channel_state = channel.check_connectivity_state(true);
                match channel_state {
                    ConnectivityState::GRPC_CHANNEL_CONNECTING => {
                        debug!("the current connection is connecting to grpc server");
                        service_state
                            .store(GrpcClientState::Unhealthy.state_code(), Ordering::Release);
                        let deadline = Duration::from_secs(5);
                        disconnect_notifier.notify_waiters();
                        channel.wait_for_connected(deadline).await;
                    }
                    ConnectivityState::GRPC_CHANNEL_READY => {
                        debug!("the current connection state is in ready");
                        let current_channel_state = service_state.load(Ordering::Relaxed);
                        if GrpcClientState::Healthy.state_code() != current_channel_state {
                            service_state
                                .store(GrpcClientState::Healthy.state_code(), Ordering::SeqCst);

                            // notify
                            reconnect_notifier.notify_waiters();
                            // send event
                            let event = GrpcReconnectedEvent {};
                            event_bus::post(Box::new(event));
                        }
                        let deadline = Duration::from_secs(5);
                        channel
                            .wait_for_state_change(ConnectivityState::GRPC_CHANNEL_READY, deadline)
                            .await;
                    }
                    ConnectivityState::GRPC_CHANNEL_TRANSIENT_FAILURE => {
                        debug!("the current connection state is in transient_failure");
                        service_state
                            .store(GrpcClientState::Unhealthy.state_code(), Ordering::Release);
                        let deadline = Duration::from_secs(5);
                        disconnect_notifier.notify_waiters();
                        channel.wait_for_connected(deadline).await;
                    }
                    ConnectivityState::GRPC_CHANNEL_IDLE => {
                        debug!("the current connection state is in idle");
                        channel.check_connectivity_state(true);
                        let deadline = Duration::from_secs(5);
                        channel
                            .wait_for_state_change(ConnectivityState::GRPC_CHANNEL_IDLE, deadline)
                            .await;
                    }
                    ConnectivityState::GRPC_CHANNEL_SHUTDOWN => {
                        debug!("grpc server has already shutdown!");
                        service_state
                            .store(GrpcClientState::Shutdown.state_code(), Ordering::Release);
                        disconnect_notifier.notify_waiters();
                        reconnect_notifier.notify_waiters();
                        break;
                    }
                }
            }
        };

        executor::spawn(check_task);
    }
}

enum GrpcClientState {
    Healthy,
    Unhealthy,
    Shutdown,
}

impl GrpcClientState {
    fn state_code(&self) -> i8 {
        match self {
            Self::Healthy => 0,
            Self::Unhealthy => 1,
            Self::Shutdown => 2,
        }
    }
}

impl From<i8> for GrpcClientState {
    fn from(code: i8) -> Self {
        match code {
            0 => Self::Healthy,
            1 => Self::Unhealthy,
            2 => Self::Shutdown,
            _ => panic!("illegal state code, {}", code),
        }
    }
}
