//! Crate-wide fixtures shared by unit tests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

/// Blocks inside the callback until the test releases it.
pub(crate) struct BlockingListener {
    pub(crate) entered: Arc<AtomicBool>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl BlockingListener {
    pub(crate) fn new() -> (Arc<Self>, mpsc::Sender<()>) {
        let (release_tx, release_rx) = mpsc::channel();
        let listener = Arc::new(Self {
            entered: Arc::new(AtomicBool::new(false)),
            release: Mutex::new(release_rx),
        });
        (listener, release_tx)
    }

    fn block(&self) {
        self.entered.store(true, Ordering::Release);
        let _ = self.release.lock().expect("release receiver").recv();
    }
}

/// Records that a callback ran.
pub(crate) struct SignalingListener(pub(crate) Arc<AtomicBool>);

#[cfg(feature = "config")]
impl crate::api::config::ConfigChangeListener for BlockingListener {
    fn notify(&self, _: Arc<crate::api::config::ConfigResponse>) {
        self.block();
    }
}

#[cfg(feature = "naming")]
impl crate::api::naming::NamingEventListener for BlockingListener {
    fn event(&self, _: Arc<crate::api::naming::NamingChangeEvent>) {
        self.block();
    }
}

#[cfg(feature = "config")]
impl crate::api::config::ConfigChangeListener for SignalingListener {
    fn notify(&self, _: Arc<crate::api::config::ConfigResponse>) {
        self.0.store(true, Ordering::Release);
    }
}

#[cfg(feature = "naming")]
impl crate::api::naming::NamingEventListener for SignalingListener {
    fn event(&self, _: Arc<crate::api::naming::NamingChangeEvent>) {
        self.0.store(true, Ordering::Release);
    }
}
