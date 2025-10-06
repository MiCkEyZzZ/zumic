pub mod config;
mod filters;
mod formatter;
pub mod sinks;

use config::LoggingConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

/// Handle для управления lifecycle логирования.
pub struct LoggingHandle {
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl LoggingHandle {
    /// Принудительный flush всех буферов.
    pub fn flush(&self) {
        // WorkerGuard автоматически делает flush при drop
    }

    /// Graceful shutdown с явным flush.
    pub fn shutdown(self) {
        drop(self);
    }
}

/// Инициализация логирования с конфигурацией
pub fn init_logging(
    mut config: LoggingConfig
) -> Result<LoggingHandle, Box<dyn std::error::Error>> {
    // Применяем env overrides
    config.apply_env_overrides();

    // Валидация
    config.validate()?;

    // Создать директорию
    config.ensure_log_dir()?;

    // Build filter
    let env_filter = filters::build_filter_from_config(&config);

    // Layers
    let mut layers: Vec<Box<dyn Layer<_> + Send + Sync>> = Vec::new();

    // Console layer
    if config.console_enabled && config.console.enabled {
        let console_layer = sinks::console::layer_with_config(&config)?;
        layers.push(Box::new(console_layer));
    }

    // File layer
    let file_guard = if config.file_enabled && config.file.enabled {
        let (file_layer, guard) = sinks::file::layer_with_config(&config)?;
        layers.push(Box::new(file_layer));
        Some(guard)
    } else {
        None
    };

    // Initialize subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(layers)
        .init();

    // Log startup
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_level = %config.level,
        log_dir = %config.log_dir.display(),
        "Logging system initialized"
    );

    Ok(LoggingHandle {
        _file_guard: file_guard,
    })
}

/// Старая функция для обратной совместимости
pub fn init_logging_simple() {
    let config = LoggingConfig::default();
    if let Err(e) = init_logging(config) {
        eprintln!("Failed to initialize logging: {}", e);
    }
}
