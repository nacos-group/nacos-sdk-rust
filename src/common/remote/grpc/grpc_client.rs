use std::sync::atomic::{AtomicI8, Ordering};
use std::{sync::Arc, time::Duration};

use grpcio::{CallOption, Channel, ChannelBuilder, ConnectivityState, Environment, LbPolicy};

use tracing::{debug, error, info, instrument, warn, Instrument};

use crate::api::error::Result;
use crate::common::remote::grpc::events::ReconnectedEvent;
use crate::common::remote::grpc::events::{DisconnectEvent, ShutdownEvent};
use crate::{api::error::Error::ClientUnhealthy, api::error::Error::ErrResult, common::event_bus};
use crate::{api::error::Error::GrpcioJoin, nacos_proto::v2::Payload};
use crate::{
    common::executor,
    nacos_proto::v2::{BiRequestStreamClient, RequestClient},
};

use super::bi_channel::{BiChannel, ResponseWriter};
use super::message::GrpcMessage;
use super::message::GrpcMessageData;

pub(crate) struct GrpcClient {
    request_client: RequestClient,
    bi_request_stream_client: BiRequestStreamClient,
    client_state: Arc<AtomicI8>,
    client_id: String,
}

impl GrpcClient {
    // #[instrument(fields(client_id = client_id), skip_all)]
    pub(crate) async fn new(address: &str, client_id: String) -> Result<Self> {
        info!("init grpc client: {address}");
        let env = Arc::new(Environment::new(1));
        // TODO grpc keep-alive configuration should be customized by users rather than default values
        let grpc_channel = ChannelBuilder::new(env)
            .keepalive_time(Duration::from_secs(60 * 6))
            .keepalive_timeout(Duration::from_secs(20))
            // LbPolicy::PickFirst, It is a sequential attempt to link, so need reorder address to balance the load as much as possible. pls see https://github.com/grpc/grpc/blob/master/doc/load-balancing.md
            .load_balancing_policy(LbPolicy::PickFirst)
            .use_local_subchannel_pool(true) // same target-addr build multi sub-channel, independent link, not reused.
            .connect(address);

        let deadline = Duration::from_secs(10);
        let is_connect = grpc_channel.wait_for_connected(deadline).await;
        if !is_connect {
            return Err(ClientUnhealthy("can't connect target server, please check network or the server address if it's wrong.".to_string()));
        }

        let request_client = RequestClient::new(grpc_channel.clone());

        let bi_channel = grpc_channel.clone();
        let bi_request_stream_client = BiRequestStreamClient::new(bi_channel);

        let client = GrpcClient {
            request_client,
            bi_request_stream_client,
            client_state: Arc::new(AtomicI8::new(GrpcClientState::Healthy.state_code())),
            client_id,
        };

        client.health_check(grpc_channel);
        Ok(client)
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn shutdown(&mut self) {
        self.client_state.store(
            GrpcClientState::Shutdown.into(),
            std::sync::atomic::Ordering::Release,
        );
        info!("grpc client shutdown.");
    }

    pub(crate) fn is_shutdown(&self) -> bool {
        let state: GrpcClientState = self.client_state.load(Ordering::Acquire).into();
        matches!(state, GrpcClientState::Shutdown)
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn open_bi_channel<F>(&self, processor: F) -> Result<BiChannel>
    where
        F: Fn(Payload, ResponseWriter) + Send + Sync + 'static,
    {
        let call_opt = CallOption::default().wait_for_ready(true);
        let bi_stream = self
            .bi_request_stream_client
            .request_bi_stream_opt(call_opt);

        if let Err(e) = bi_stream {
            error!("request bi stream occur an error. {e:?}");
            return Err(ErrResult("request bi stream occur an error.".to_string()));
        }

        let bi_stream = bi_stream.unwrap();

        let bi_channel = BiChannel::new(bi_stream, Arc::new(processor), self.client_id.clone());

        Ok(bi_channel)
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn unary_call_async<R, P>(
        &self,
        message: GrpcMessage<R>,
        time_out: Duration,
    ) -> Result<GrpcMessage<P>>
    where
        R: GrpcMessageData,
        P: GrpcMessageData,
    {
        let request_payload = message.into_payload();

        if let Err(e) = request_payload {
            error!("unary_call_async request grpc message convert to payload occur an error:{e:?}");
            return Err(e);
        }
        let request_payload = request_payload.unwrap();

        let call_option = CallOption::default().timeout(time_out);
        let response_payload = self
            .request_client
            .request_async_opt(&request_payload, call_option);

        if let Err(error) = response_payload {
            error!("unary_call_async send grpc messages occur an error. {error:?}");
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap().await;

        if let Err(error) = response_payload {
            error!("unary_call_async receive grpc messages occur an error. {error:?}");
            return Err(GrpcioJoin(error));
        }

        let response_payload = response_payload.unwrap();

        let message = GrpcMessage::<P>::from_payload(response_payload)?;
        Ok(message)
    }

    #[instrument(parent = None , fields(client_id = &self.client_id), skip_all)]
    fn health_check(&self, grpc_channel: Channel) {
        let client_id = self.client_id.clone();
        let client_state = self.client_state.clone();
        let check_task = async move {
            loop {
                let current_state = client_state.load(std::sync::atomic::Ordering::Acquire);
                let current_state = GrpcClientState::from(current_state);

                if current_state == GrpcClientState::Shutdown {
                    debug!("the grpc client has already shutdown!");
                    break;
                }

                let channel_state = grpc_channel.check_connectivity_state(true);

                // deadline wait 10 minutes
                let deadline = Duration::from_secs(60 * 10);

                match channel_state {
                    ConnectivityState::GRPC_CHANNEL_CONNECTING => {
                        debug!("the current grpc connection is connecting to grpc server");
                        let ret = client_state.compare_exchange(
                            current_state.state_code(),
                            GrpcClientState::Unhealthy.into(),
                            Ordering::SeqCst,
                            Ordering::Acquire,
                        );
                        if let Err(e) = ret {
                            error!("update grpc client state failed, the current state has already changed. {:?} -> {:?}", current_state, GrpcClientState::from(e));
                            break;
                        }

                        grpc_channel.wait_for_connected(deadline).await;
                    }
                    ConnectivityState::GRPC_CHANNEL_READY => {
                        debug!("the current grpc connection state is in ready");
                        if GrpcClientState::Healthy != current_state {
                            // notify
                            let ret = client_state.compare_exchange(
                                current_state.state_code(),
                                GrpcClientState::Healthy.into(),
                                Ordering::SeqCst,
                                Ordering::Acquire,
                            );
                            if let Err(e) = ret {
                                error!("update grpc client state failed, the current state has already changed. {:?} -> {:?}", current_state, GrpcClientState::from(e));
                                break;
                            }

                            debug!("the connection is already reconnect!");
                            // send event
                            event_bus::post(Arc::new(ReconnectedEvent {
                                scope: client_id.clone()
                            }));
                        }

                        grpc_channel
                            .wait_for_state_change(ConnectivityState::GRPC_CHANNEL_READY, deadline)
                            .await;
                    }
                    ConnectivityState::GRPC_CHANNEL_TRANSIENT_FAILURE => {
                        error!("the current grpc connection state is in transient_failure");
                        // send event
                        event_bus::post(Arc::new(DisconnectEvent {
                            scope: client_id.clone()
                        }));

                        let ret = client_state.compare_exchange(
                            current_state.state_code(),
                            GrpcClientState::Unhealthy.into(),
                            Ordering::SeqCst,
                            Ordering::Acquire,
                        );
                        if let Err(e) = ret {
                            error!("update grpc client state failed, the current state has already changed. {:?} -> {:?}", current_state, GrpcClientState::from(e));
                            break;
                        }

                        grpc_channel.wait_for_connected(deadline).await;
                    }
                    ConnectivityState::GRPC_CHANNEL_IDLE => {
                        debug!("the current grpc connection state is in idle");
                        grpc_channel
                            .wait_for_state_change(ConnectivityState::GRPC_CHANNEL_IDLE, deadline)
                            .await;
                    }
                    ConnectivityState::GRPC_CHANNEL_SHUTDOWN => {
                        warn!("the grpc server has already shutdown!");
                        client_state.store(
                            GrpcClientState::Shutdown.into(),
                            std::sync::atomic::Ordering::Release,
                        );
                        break;
                    }
                }
            }
            event_bus::post(Arc::new(ShutdownEvent {
                scope: client_id.clone()
            }));
            warn!("health_check_task quit!");
        }.in_current_span();
        executor::spawn(check_task);
    }
}

impl Drop for GrpcClient {
    fn drop(&mut self) {
        self.client_state.store(
            GrpcClientState::Shutdown.into(),
            std::sync::atomic::Ordering::Release,
        );
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
            _ => panic!("illegal state code={code}"),
        }
    }
}

impl PartialEq for GrpcClientState {
    fn eq(&self, other: &Self) -> bool {
        self.state_code() == other.state_code()
    }
}

impl From<GrpcClientState> for i8 {
    fn from(state: GrpcClientState) -> Self {
        match state {
            GrpcClientState::Healthy => 0,
            GrpcClientState::Unhealthy => 1,
            GrpcClientState::Shutdown => 2,
        }
    }
}
