use std::{collections::HashMap, pin::Pin, sync::Arc, task::Poll, time::Duration};

use async_stream::stream;
use futures::Future;
use tokio::time::sleep;
use tower::{Layer, Service};
use tracing::{debug, debug_span, Instrument};

use crate::{
    api::{
        error::Error,
        plugin::{AuthContext, AuthPlugin},
    },
    common::{
        executor,
        remote::grpc::nacos_grpc_service::{
            DynamicBiStreamingCallService, DynamicUnaryCallService, GrpcStream,
        },
    },
    nacos_proto::v2::{Metadata, Payload},
};

pub(crate) struct AuthLayer {
    auth_plugin: Arc<dyn AuthPlugin>,
}

impl AuthLayer {
    pub(crate) fn new(
        auth_plugin: Arc<dyn AuthPlugin>,
        server_list: Vec<String>,
        auth_params: HashMap<String, String>,
        id: String,
    ) -> Self {
        AuthLayer::login_task(auth_plugin.clone(), server_list, auth_params, id);
        Self { auth_plugin }
    }

    fn login_task(
        auth_plugin: Arc<dyn AuthPlugin>,
        server_list: Vec<String>,
        auth_params: HashMap<String, String>,
        id: String,
    ) {
        let _span_enter = debug_span!("auth_task", id = id).entered();
        let auth_context = AuthContext::default().add_params(auth_params);
        // auth_plugin.login(server_list.clone(), auth_context.clone());
        executor::spawn(
            async move {
                loop {
                    auth_plugin
                        .login(server_list.clone(), auth_context.clone())
                        .await;
                    debug!("auth_plugin schedule at fixed delay");
                    sleep(Duration::from_secs(30)).await;
                }
            }
            .in_current_span(),
        );
    }
}

struct AuthUnaryCallService {
    auth_plugin: Arc<dyn AuthPlugin>,
    inner: DynamicUnaryCallService,
}

impl Service<Payload> for AuthUnaryCallService {
    type Response = Payload;

    type Error = Error;

    type Future = Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send + 'static>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Payload) -> Self::Future {
        let login_identity = self.auth_plugin.get_login_identity();
        let contexts = login_identity.contexts;

        let metadata = req.metadata.take();
        let metadata = if let Some(mut metadata) = metadata {
            metadata.headers.extend(contexts);
            Some(metadata)
        } else {
            let metadata = Metadata {
                headers: contexts,
                ..Default::default()
            };
            Some(metadata)
        };
        req.metadata = metadata;

        self.inner.call(req)
    }
}

struct AuthBiStreamingCallService {
    auth_plugin: Arc<dyn AuthPlugin>,
    inner: DynamicBiStreamingCallService,
}

impl Service<GrpcStream<Payload>> for AuthBiStreamingCallService {
    type Response = GrpcStream<Result<Payload, Error>>;

    type Error = Error;

    type Future = Pin<
        Box<
            dyn Future<Output = Result<GrpcStream<Result<Payload, Error>>, Error>> + Send + 'static,
        >,
    >;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: GrpcStream<Payload>) -> Self::Future {
        let auth_plugin = self.auth_plugin.clone();
        let stream = stream! {
            for await mut value in req {
                let login_identity = auth_plugin.get_login_identity();
                let contexts = login_identity.contexts;

                let metadata = value.metadata.take();
                let metadata = if let Some(mut metadata) = metadata {
                    metadata.headers.extend(contexts);
                    Some(metadata)
                } else {
                    let metadata = Metadata{
                        headers: contexts,
                        ..Default::default()
                    };
                    Some(metadata)
                };
                value.metadata = metadata;

                yield value;
            }
        };

        let stream = GrpcStream::new(Box::pin(stream));

        self.inner.call(stream)
    }
}

impl Layer<DynamicUnaryCallService> for AuthLayer {
    type Service = DynamicUnaryCallService;

    fn layer(&self, inner: DynamicUnaryCallService) -> Self::Service {
        let service = AuthUnaryCallService {
            auth_plugin: self.auth_plugin.clone(),
            inner,
        };
        Box::new(service)
    }
}

impl Layer<DynamicBiStreamingCallService> for AuthLayer {
    type Service = DynamicBiStreamingCallService;

    fn layer(&self, inner: DynamicBiStreamingCallService) -> Self::Service {
        let service = AuthBiStreamingCallService {
            auth_plugin: self.auth_plugin.clone(),
            inner,
        };
        Box::new(service)
    }
}
