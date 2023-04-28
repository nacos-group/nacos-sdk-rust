use std::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::api::error::Error;
use crate::api::error::Error::NoAvailableServer;
use futures::Future;
use tower::Service;

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
            .map(|server| {
                server
                    .rsplit_once(":")
                    .map(|(addr, port)| (addr.split_once("://").map(|v| v.1).unwrap_or(addr), port))
                    .map(|(addr, port)| vec![addr.to_owned(), port.to_owned()])
                    .unwrap_or_default()
            })
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
            server_list,
            index: 0,
        }
    }
}

impl Service<()> for PollingServerListService {
    type Response = (String, u32);

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
        let server_list = self.server_list.get(self.index);
        let server_list = if let Some(server_list) = server_list {
            Ok(server_list.clone())
        } else {
            Err(NoAvailableServer)
        };
        Box::pin(async move { server_list })
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
        debug!("ip:{}, port:{}", server1.0, server1.1);

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server2 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server2.0, server2.1);

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server3 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server3.0, server3.1);

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server4 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server4.0, server4.1);

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server5 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server5.0, server5.1);

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server6 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server6.0, server6.1);

        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
        let server7 = service.call(()).await.unwrap();
        debug!("ip:{}, port:{}", server7.0, server7.1);
    }
}
