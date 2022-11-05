use std::{sync::Arc, time::Duration};

use futures::SinkExt;
use futures::TryStreamExt;
use grpcio::{
    CallOption, Channel, ChannelBuilder, ClientDuplexReceiver, ConnectivityState, Environment,
    LbPolicy, StreamingCallSink, WriteFlags,
};
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    watch, Mutex, RwLock,
};
use tracing::{debug, error, info, warn};

use crate::api::error::Result;
use crate::common::remote::grpc::events::GrpcConnectHealthCheckEvent;
use crate::common::remote::grpc::events::GrpcReconnectedEvent;
use crate::{api::error::Error::ClientUnhealthy, api::error::Error::ErrResult, common::event_bus};
use crate::{api::error::Error::GrpcioJoin, nacos_proto::v2::Payload};
use crate::{
    common::executor,
    nacos_proto::v2::{BiRequestStreamClient, RequestClient},
};

use super::message::GrpcMessage;
use super::message::GrpcMessageData;

pub(crate) struct GrpcClient {
    request_client: RequestClient,
    bi_request_stream_client: Arc<BiRequestStreamClient>,
    bi_request_sender: Arc<RwLock<Option<Sender<Result<Payload>>>>>,
    grpc_client_state_sender: Arc<watch::Sender<GrpcClientState>>,
}

impl GrpcClient {
    pub(crate) async fn new(address: &str) -> Result<Self> {
        let address = crate::common::remote::into_grpc_server_addr(address);
        let address = address.as_str();
        info!("init grpc client: {}", address);
        let env = Arc::new(Environment::new(2));
        let grpc_channel = ChannelBuilder::new(env)
            .load_balancing_policy(LbPolicy::PickFirst)
            .connect(address);

        let deadline = Duration::from_secs(3);
        let is_connect = grpc_channel.wait_for_connected(deadline).await;
        if !is_connect {
            return Err(ClientUnhealthy("can't connect target server, please check network or the server address if it's wrong.".to_string()));
        }

        let request_client = RequestClient::new(grpc_channel.clone());

        let bi_channel = grpc_channel.clone();
        let bi_request_stream_client = Arc::new(BiRequestStreamClient::new(bi_channel));

        let (grpc_client_state_sender, _) =
            watch::channel::<GrpcClientState>(GrpcClientState::Healthy);

        let client = GrpcClient {
            request_client,
            bi_request_stream_client,
            grpc_client_state_sender: Arc::new(grpc_client_state_sender),
            bi_request_sender: Arc::new(RwLock::new(None)),
        };

        client.health_check(grpc_channel);
        Ok(client)
    }

    pub(crate) async fn shutdown(&mut self) {
        while !self.grpc_client_state_sender.is_closed() {
            let _ = self
                .grpc_client_state_sender
                .send_replace(GrpcClientState::Shutdown);
        }
        info!("grpc client shutdown.");
    }

    async fn streaming_send_task(
        mut grpc_client_state_receiver: watch::Receiver<GrpcClientState>,
        request_receiver: Arc<Mutex<Receiver<Result<Payload>>>>,
        mut response_sender: StreamingCallSink<Payload>,
    ) {
        let mut request_receiver = request_receiver.lock().await;

        while { grpc_client_state_receiver.borrow_and_update() }.state_code()
            == GrpcClientState::Healthy.state_code()
        {
            let payload = request_receiver.recv().await;
            if payload.is_none() {
                error!("the bi request channel has already been closed. streaming_send_task quit!");
                break;
            }

            let payload = payload.unwrap();

            if let Err(e) = payload {
                error!("the bi request channel receive an error message, close the bi request channel and streaming_send_task quit.{:?}", e);
                request_receiver.close();
                while (request_receiver.recv().await).is_some() {}
                break;
            }

            let payload = payload.unwrap();
            let send_ret = response_sender.send((payload, WriteFlags::default())).await;
            if let Err(error) = send_ret {
                error!(
                    "send a grpc message to the server occur an error. {:?}",
                    error
                );
            }
        }

        error!("grpc client state is not healthy, close the bi request channel.");
        request_receiver.close();
        while (request_receiver.recv().await).is_some() {}

        warn!("streaming_send_task  quit");
    }

    async fn streaming_receive_task(
        mut grpc_client_state_receiver: watch::Receiver<GrpcClientState>,
        mut request_receiver: ClientDuplexReceiver<Payload>,
        response_sender: Arc<Sender<Result<Payload>>>,
    ) {
        while { grpc_client_state_receiver.borrow_and_update() }.state_code()
            == GrpcClientState::Healthy.state_code()
        {
            let payload = request_receiver.try_next().await;
            if let Err(e) = payload {
                error!(
                    "the bi response channel receive grpc messages from the server occur an error, streaming_receive_task quit. {:?}",
                    e
                );
                break;
            }

            let payload = payload.unwrap();
            if payload.is_none() {
                error!(
                    "the bi response channel has already been closed, streaming_receive_task quit."
                );
                break;
            }

            let payload = payload.unwrap();

            let send_ret = response_sender.send(Ok(payload)).await;
            if let Err(error) = send_ret {
                error!(
                    "forward grpc messages to the client occur an error. {:?}",
                    error
                );
            }
        }
        warn!("streaming_receive_task quit");
    }

    async fn duplex_task(
        grpc_client_state_sender: Arc<watch::Sender<GrpcClientState>>,
        bi_request_stream_client: Arc<BiRequestStreamClient>,
        client_request_sender: Arc<Sender<Result<Payload>>>,
        client_request_receiver: Receiver<Result<Payload>>,
        client_response_sender: Sender<Result<Payload>>,
    ) {
        let client_request_receiver = Arc::new(Mutex::new(client_request_receiver));
        let client_response_sender = Arc::new(client_response_sender);

        let mut grpc_client_state_receiver = grpc_client_state_sender.subscribe();
        loop {
            let current_grpc_client_state =
                { grpc_client_state_receiver.borrow_and_update().clone() };
            match current_grpc_client_state {
                GrpcClientState::Unhealthy => {
                    warn!("the current grpc client is in UNHEALTHY, duplex_task quit.");
                    break;
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

            let send_task_handler = executor::spawn(Self::streaming_send_task(
                grpc_client_state_sender.subscribe(),
                client_request_receiver.clone(),
                duplex_sink,
            ));
            let receive_task_handler = executor::spawn(Self::streaming_receive_task(
                grpc_client_state_sender.subscribe(),
                duplex_receiver,
                client_response_sender.clone(),
            ));

            let receive_state = grpc_client_state_receiver.changed().await;
            debug!(
                "grpc_client_state_receiver receive state {:?}",
                receive_state
            );

            let _ = client_response_sender
                .send(Err(ClientUnhealthy("client unhealthy".to_string())))
                .await;
            let _ = client_request_sender
                .send(Err(ClientUnhealthy("client unhealthy".to_string())))
                .await;
            let _ = send_task_handler.await;
            let _ = receive_task_handler.await;

            debug!("all the task hash already abort");
            debug!("duplex_task quit");
        }
    }

    pub(crate) async fn streaming_call(
        &self,
    ) -> (Arc<Sender<Result<Payload>>>, Receiver<Result<Payload>>) {
        let (request_sender, request_receiver) = channel::<Result<Payload>>(1024);
        let (rsp_sender, rsp_receiver) = channel::<Result<Payload>>(1024);

        let bi_request_sender = request_sender.clone();
        let request_sender = Arc::new(request_sender);
        let request_sender_for_return = request_sender.clone();

        let grpc_client_state_sender = self.grpc_client_state_sender.clone();

        let bi_request_stream_client = self.bi_request_stream_client.clone();

        executor::spawn(async move {
            Self::duplex_task(
                grpc_client_state_sender,
                bi_request_stream_client,
                request_sender.clone(),
                request_receiver,
                rsp_sender,
            )
            .await;
        });

        {
            let mut lock_bi_request_sender = self.bi_request_sender.write().await;
            *lock_bi_request_sender = Some(bi_request_sender);
        }
        (request_sender_for_return, rsp_receiver)
    }

    pub(crate) async fn bi_call<R>(&self, message: GrpcMessage<R>) -> Result<()>
    where
        R: GrpcMessageData,
    {
        let current_grpc_client_state = { self.grpc_client_state_sender.borrow().clone() };
        match current_grpc_client_state {
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
        let payload = message.into_payload();
        if payload.is_err() {
            let error = payload.unwrap_err();
            error!("bi call send message occur an error:{:?}", error);
            return Err(error);
        }
        let payload = payload.unwrap();

        let bi_sender_lock = self.bi_request_sender.read().await;
        if bi_sender_lock.is_none() {
            warn!("bi call send message failed. cannot get lock.");
            return Ok(());
        }
        let bi_sender = bi_sender_lock.as_ref().unwrap();
        let ret = bi_sender.send(Ok(payload)).await;
        if let Err(e) = ret {
            error!("bi call send message occur an error. {:?}", e);
            return Err(ErrResult(
                "bi call send message occur an error.".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn unary_call_async<R, P>(
        &self,
        message: GrpcMessage<R>,
    ) -> Result<GrpcMessage<P>>
    where
        R: GrpcMessageData,
        P: GrpcMessageData,
    {
        let current_grpc_client_state = { self.grpc_client_state_sender.borrow().clone() };
        match current_grpc_client_state {
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
            error!(
                "unary_call_async request grpc message convert to payload occur an error:{:?}",
                error
            );
            return Err(error);
        }
        let request_payload = request_payload.unwrap();

        let response_payload = self.request_client.request_async(&request_payload);

        if let Err(error) = response_payload {
            error!(
                "unary_call_async send grpc messages occur an error. {:?}",
                error
            );
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap().await;

        if let Err(error) = response_payload {
            error!(
                "unary_call_async receive grpc messages occur an error. {:?}",
                error
            );
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap();

        let message = GrpcMessage::<P>::from_payload(response_payload);
        if let Err(error) = message {
            error!(
                "unary_call_async response grpc message convert to message object occur an error. {:?}",
                error
            );
            return Err(error);
        }
        Ok(message.unwrap())
    }

    fn health_check(&self, grpc_channel: Channel) {
        let grpc_client_state_sender = self.grpc_client_state_sender.clone();
        let check_task = async move {
            loop {
                let current_grpc_client_state = { grpc_client_state_sender.borrow().clone() };
                if GrpcClientState::Shutdown.state_code() == current_grpc_client_state.state_code()
                {
                    debug!("the grpc server has already shutdown!");
                    break;
                }

                let channel_state = grpc_channel.check_connectivity_state(true);

                match channel_state {
                    ConnectivityState::GRPC_CHANNEL_CONNECTING => {
                        debug!("the current grpc connection is connecting to grpc server");
                        let is_modified =
                            grpc_client_state_sender.send_if_modified(|previous_state| {
                                if GrpcClientState::Shutdown.state_code()
                                    == previous_state.state_code()
                                {
                                    return false;
                                }
                                *previous_state = GrpcClientState::Unhealthy;
                                true
                            });
                        if !is_modified {
                            break;
                        }
                        let deadline = Duration::from_secs(5);
                        grpc_channel.wait_for_connected(deadline).await;
                    }
                    ConnectivityState::GRPC_CHANNEL_READY => {
                        debug!("the current grpc connection state is in ready");
                        if GrpcClientState::Healthy.state_code()
                            != current_grpc_client_state.state_code()
                        {
                            // notify
                            let is_modified =
                                grpc_client_state_sender.send_if_modified(|previous_state| {
                                    if GrpcClientState::Shutdown.state_code()
                                        == previous_state.state_code()
                                    {
                                        return false;
                                    }
                                    *previous_state = GrpcClientState::Healthy;
                                    true
                                });
                            if !is_modified {
                                break;
                            }

                            // send event
                            let event = GrpcReconnectedEvent {};
                            event_bus::post(Box::new(event));
                        } else {
                            // health check
                            event_bus::post(Box::new(GrpcConnectHealthCheckEvent {}));
                        }
                        let deadline = Duration::from_secs(5);
                        grpc_channel
                            .wait_for_state_change(ConnectivityState::GRPC_CHANNEL_READY, deadline)
                            .await;
                    }
                    ConnectivityState::GRPC_CHANNEL_TRANSIENT_FAILURE => {
                        debug!("the current grpc connection state is in transient_failure");
                        let is_modified =
                            grpc_client_state_sender.send_if_modified(|previous_state| {
                                if GrpcClientState::Shutdown.state_code()
                                    == previous_state.state_code()
                                {
                                    return false;
                                }
                                *previous_state = GrpcClientState::Unhealthy;
                                true
                            });
                        if !is_modified {
                            break;
                        }
                        let deadline = Duration::from_secs(5);
                        grpc_channel.wait_for_connected(deadline).await;
                    }
                    ConnectivityState::GRPC_CHANNEL_IDLE => {
                        debug!("the current grpc connection state is in idle");
                        event_bus::post(Box::new(GrpcConnectHealthCheckEvent {}));
                        let deadline = Duration::from_secs(5);
                        grpc_channel
                            .wait_for_state_change(ConnectivityState::GRPC_CHANNEL_IDLE, deadline)
                            .await;
                    }
                    ConnectivityState::GRPC_CHANNEL_SHUTDOWN => {
                        debug!("the grpc server has already shutdown!");
                        break;
                    }
                }
            }

            let _ = grpc_client_state_sender.send_replace(GrpcClientState::Shutdown);
            drop(grpc_client_state_sender);
            debug!("health_check_task quit!");
        };
        executor::spawn(check_task);
    }
}

#[derive(Clone, Debug)]
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
