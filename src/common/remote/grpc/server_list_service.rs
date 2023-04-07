use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicI16, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use crate::api::error::Error;
use crate::api::error::Error::NoAvailableServer;
use futures::Future;
use http::Uri;
use tower::Service;

pub(crate) struct PollingServerListService {
    server_list: Arc<Vec<Uri>>,
    index: Arc<AtomicI16>,
}

impl PollingServerListService {
    pub(crate) fn new(server_list: Vec<String>) -> Self {
        if server_list.is_empty() {
            panic!("server list must not empty");
        }
        let server_list: Vec<Uri> = server_list
            .into_iter()
            .map(|server| server.try_into().unwrap())
            .collect();

        Self {
            server_list: Arc::new(server_list),
            index: Arc::new(AtomicI16::new(0)),
        }
    }
}

impl Service<()> for PollingServerListService {
    type Response = Uri;

    type Error = Error;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: ()) -> Self::Future {
        let server_list = self.server_list.clone();
        let index = self.index.clone();

        let server_info_fut = async move {
            loop {
                let current_index = index.fetch_add(1, Ordering::SeqCst);
                if current_index >= server_list.len() as i16 {
                    let _ = index.compare_exchange(
                        current_index + 1,
                        0,
                        Ordering::SeqCst,
                        Ordering::Acquire,
                    );
                    continue;
                }
                let server_uri = server_list.get(current_index as usize);
                let ret = match server_uri {
                    Some(uri) => Ok(uri.clone()),
                    None => Err(NoAvailableServer),
                };
                return ret;
            }
        };
        Box::pin(server_info_fut)
    }
}
