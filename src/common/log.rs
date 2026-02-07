use std::str::FromStr;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// 定义一个全局的 OnceLock 来持有 guards
#[allow(dead_code)]
pub static LOG_GUARD: std::sync::OnceLock<Vec<WorkerGuard>> = std::sync::OnceLock::new();

/// 初始化日志
#[allow(dead_code)]
pub fn init(log_path: String, log_level: String) {
    // let log_path = crate::util::HOME_DIR.as_path().to_str().unwrap().to_owned();
    // crate::util::log::init(log_path, "INFO".to_string());
    LOG_GUARD.get_or_init(|| init_logging(log_path, log_level));
}

#[allow(dead_code)]
fn init_logging(log_path: String, log_level: String) -> Vec<WorkerGuard> {
    let log_level = LevelFilter::from_str(&log_level).unwrap_or(LevelFilter::INFO);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("INFO"));

    // Setup stdout/files layer
    let mut layers = Vec::with_capacity(3);
    let mut guards = Vec::with_capacity(3);

    if cfg!(debug_assertions) {
        // 创建控制台日志记录器（debug 模式才打印）
        let (console_writer, console_guard) = tracing_appender::non_blocking(std::io::stdout());
        let console_layer = fmt::layer()
            .with_writer(console_writer)
            .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
            .with_ansi(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_filter(log_level);
        layers.push(console_layer);
        guards.push(console_guard);
    }

    // 创建按日滚动的文件日志记录器 1
    let file_appender1 =
        RollingFileAppender::new(Rotation::DAILY, log_path.clone(), "nacos-common.log");
    let (non_blocking1, file_guard1) = tracing_appender::non_blocking(file_appender1);
    let common_file_layer = fmt::layer()
        .with_writer(non_blocking1)
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .with_ansi(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_filter(log_level);
    layers.push(common_file_layer);
    guards.push(file_guard1);

    // 创建按日滚动的文件日志记录器 2
    let file_appender2 = RollingFileAppender::new(Rotation::DAILY, log_path, "nacos-error.log");
    let (non_blocking2, file_guard2) = tracing_appender::non_blocking(file_appender2);
    let err_file_layer = fmt::layer()
        .with_writer(non_blocking2)
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .with_ansi(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_filter(LevelFilter::ERROR);
    layers.push(err_file_layer);
    guards.push(file_guard2);

    let subscriber = tracing_subscriber::registry().with(env_filter).with(layers);

    let _result = subscriber.try_init();
    if _result.is_err() {
        tracing::error!("Failed to set nacos-sdk subscriber, please config and set it.");
    }

    guards
}
