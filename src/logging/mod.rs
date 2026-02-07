pub mod config;
mod filters;
pub mod formats;
mod formatter;
pub mod handle;
pub mod sinks;
pub mod slow_log;
pub mod slow_query_layer;

pub use config::LoggingConfig;
pub use handle::LoggingHandle;
pub use slow_log::{SlowLogConfig, SlowLogStats, SlowQueryTracker};
pub use slow_query_layer::SlowQueryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

/// Инициализация логирования с конфигурацией
pub fn init_logging(
    mut config: LoggingConfig
) -> Result<LoggingHandle, Box<dyn std::error::Error>> {
    config.apply_env_overrides();
    config.validate()?;
    config.ensure_log_dir()?;

    let env_filter = filters::build_filter_from_config(&config);
    let mut layers = Vec::new();

    // Console layer
    if config.console_enabled && config.console.enabled {
        let console_layer = sinks::console::layer_with_config(&config)?;
        layers.push(console_layer);
    }

    // File layer
    let file_guard = if config.file_enabled && config.file.enabled {
        let (file_layer, guard) = sinks::file::layer_with_config(&config)?;
        layers.push(Box::new(file_layer));
        Some(guard)
    } else {
        None
    };

    // Slow query layer (NEW!)
    if config.slow_log.enabled {
        init_slow_log(&config)?;
        let slow_layer = SlowQueryLayer::new();
        layers.push(slow_layer.boxed());
    }

    // Initialize subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(layers)
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_level = %config.level,
        log_dir = %config.log_dir.display(),
        console_enabled = config.console_enabled,
        file_enabled = config.file_enabled,
        slow_log_enabled = config.slow_log.enabled,
        "Logging system initialized"
    );

    let handle = LoggingHandle::new(file_guard, None);
    Ok(handle)
}

/// Инициализация slow query logging
fn init_slow_log(config: &LoggingConfig) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    // Обновляем глобальную конфигурацию
    let slow_config = crate::logging::slow_log::SlowLogConfig {
        threshold: Duration::from_millis(config.slow_log.threshold_ms),
        sample_rate: config.slow_log.sample_rate,
        command_thresholds: config
            .slow_log
            .command_thresholds
            .iter()
            .map(|(k, v)| (k.clone(), Duration::from_millis(*v)))
            .collect(),
        enable_backtrace: config.slow_log.enable_backtrace,
        max_args_len: config.slow_log.max_args_len,
    };

    slow_log::update_config(slow_config);

    // Создаём отдельный файл для slow queries
    let slow_log_path = config.log_dir.join(&config.slow_log.filename);

    tracing::info!(
        path = %slow_log_path.display(),
        threshold_ms = config.slow_log.threshold_ms,
        sample_rate = config.slow_log.sample_rate,
        "Slow query logging initialized"
    );

    Ok(())
}

/// Старая функция для обратной совместимости
#[deprecated(note = "Use init_logging() instead")]
pub fn init_logging_simple() {
    let config = LoggingConfig::default();
    if let Err(e) = init_logging(config) {
        eprintln!("Failed to initialize logging: {e}");
    }
}
