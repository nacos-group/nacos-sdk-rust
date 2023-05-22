use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use crate::api::error::Error;
use crate::api::error::Error::NoAvailableServer;
use futures::Future;
use rand::Rng;
use tower::Service;

use super::server_address::ServerAddress;

pub(crate) struct PollingServerListService {
    server_list: Vec<(String, u32)>,
    index: usize,
}

impl PollingServerListService {
    pub(crate) fn new(server_list: Vec<String>) -> Self {
        if server_list.is_empty() {
            panic!("server list must not empty");
        }

        let server_list: Vec<(String, u32)> = server_list
            .into_iter()
            .map(|server| server.split(':').map(|data| data.to_string()).collect())
            .filter(|vec: &Vec<String>| {
                if vec.len() != 2 {
                    return false;
                }
                vec.get(0).is_some() && vec.get(1).is_some()
            })
            .filter_map(|vec| {
                let address = vec.get(0).unwrap().clone();
                let port = vec.get(1).unwrap().clone();

                let port = port.parse::<u32>();

                if let Ok(port) = port {
                    return Some((address, port));
                }
                None
            })
            .collect();
        if server_list.is_empty() {
            panic!("all the server is illegal format!");
        }

        Self {
            // random index for load balance the server list
            index: rand::thread_rng().gen_range(0..server_list.len()),
            server_list,
        }
    }
}

impl Service<()> for PollingServerListService {
    type Response = Arc<dyn ServerAddress>;

    type Error = Error;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.index += 1;
        if self.index >= self.server_list.len() {
            self.index = 0;
        }

        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: ()) -> Self::Future {
        let server_addr = self.server_list.get(self.index);
        let server_addr = if let Some((host, port)) = server_addr {
            let server_address = PollingServerAddress {
                host: host.clone(),
                port: *port,
            };
            Ok(Arc::new(server_address) as Arc<dyn ServerAddress>)
        } else {
            Err(NoAvailableServer)
        };
        Box::pin(async move { server_addr })
    }
}

struct PollingServerAddress {
    host: String,
    port: u32,
}

impl ServerAddress for PollingServerAddress {
    fn host(&self) -> String {
        self.host.clone()
    }

    fn port(&self) -> u32 {
        self.port
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
pub mod tests {
    use futures::future::poll_fn;
    use tower::Service;
    use tracing::{debug, metadata::LevelFilter};

    use super::PollingServerListService;

    #[test]
    #[should_panic(expected = "server list must not empty")]
    pub fn test_empty_server_list() {
        let _ = PollingServerListService::new(Vec::default());
    }

    #[test]
    #[should_panic(expected = "all the server is illegal format!")]
    pub fn test_illegal_format() {
        let _ = PollingServerListService::new(vec!["127.0.0.1:sd".to_string()]);
    }

    #[ignore]
    #[tokio::test]
    pub async fn test_poll_server_list() {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let mut service = PollingServerListService::new(vec![
            "127.0.0.1:8848".to_string(),
            "127.0.0.2:8848".to_string(),
            "127.0.0.3:8848".to_string(),
        ]);

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server1 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server1.host(), server1.port());

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server2 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server2.host(), server2.port());

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server3 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server3.host(), server3.port());

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server4 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server4.host(), server4.port());

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server5 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server5.host(), server5.port());

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server6 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server6.host(), server6.port());

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server7 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server7.host(), server7.port());
    }
}
