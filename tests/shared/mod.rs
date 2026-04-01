pub mod test_data;

pub use test_data::*;

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
