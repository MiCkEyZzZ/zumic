pub mod config;
mod filters;
pub mod formats;
mod formatter;
pub mod handle;
pub mod sinks;

pub use config::LoggingConfig;
pub use handle::LoggingHandle;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Инициализация логирования с конфигурацией
pub fn init_logging(
    mut config: LoggingConfig
) -> Result<LoggingHandle, Box<dyn std::error::Error>> {
    // Применяем env overrides
    config.apply_env_overrides();

    // Валидация
    config.validate()?;

    // Создать директорию логов
    config.ensure_log_dir()?;

    // Build filter
    let env_filter = filters::build_filter_from_config(&config);

    // Layers
    let mut layers: Vec<Box<dyn tracing_subscriber::Layer<_> + Send + Sync>> = Vec::new();

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

    // Логируем инициализацию
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_level = %config.level,
        log_dir = %config.log_dir.display(),
        console_enabled = config.console_enabled,
        file_enabled = config.file_enabled,
        "Logging system initialized"
    );

    // Создаём handle с guards
    let handle = LoggingHandle::new(file_guard, None);

    Ok(handle)
}

/// Старая функция для обратной совместимости
#[deprecated(note = "Use init_logging() instead")]
pub fn init_logging_simple() {
    let config = LoggingConfig::default();
    if let Err(e) = init_logging(config) {
        eprintln!("Failed to initialize logging: {}", e);
    }
}
