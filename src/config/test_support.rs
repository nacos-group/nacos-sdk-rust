use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;

use crate::api::config::{ConfigChangeListener, ConfigResponse};
use crate::api::plugin::{ConfigFilter, ConfigReq, ConfigResp};

/// A config filter that parks the notify flow until the test releases it.
pub(crate) struct BlockingFilter {
    pub(crate) entered: Arc<AtomicBool>,
    pub(crate) release: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl ConfigFilter for BlockingFilter {
    async fn filter(&self, _: Option<&mut ConfigReq>, _: Option<&mut ConfigResp>) {
        self.entered.store(true, Ordering::Release);
        self.release.notified().await;
    }
}

/// A listener that forwards every notification to a channel.
pub(crate) struct ChannelListener(pub(crate) UnboundedSender<Arc<ConfigResponse>>);

impl ConfigChangeListener for ChannelListener {
    fn notify(&self, config_resp: Arc<ConfigResponse>) {
        let _ = self.0.send(config_resp);
    }
}
