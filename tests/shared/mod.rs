pub mod test_data;

use std::sync::Once;
use tracing::level_filters::LevelFilter;

static LOGGER_INIT: Once = Once::new();

pub fn setup_log() {
    LOGGER_INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::INFO)
            .try_init();
    });
}

/// Poll `probe` up to `attempts` times, sleeping `interval` after each failed
/// probe (including the last one), and report whether any probe returned `true`.
#[allow(dead_code)]
pub async fn wait_until<F, Fut>(attempts: u32, interval: std::time::Duration, mut probe: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..attempts {
        if probe().await {
            return true;
        }
        tokio::time::sleep(interval).await;
    }
    false
}
