use std::error::Error;

/// Возвращаем boxed trait-объект — согласованно с build_formatter_from_config.
use tracing_subscriber::layer::Layer as LayerTrait;
use tracing_subscriber::registry::LookupSpan;

use crate::logging::{config::LoggingConfig, formatter};

/// Старая ф-я - базовый console layer.
pub fn layer<S>() -> impl LayerTrait<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer().with_writer(std::io::stdout as fn() -> std::io::Stdout)
}

/// Create console layer с конфигурацией
pub fn layer_with_config<S>(
    config: &LoggingConfig
) -> Result<Box<dyn LayerTrait<S> + Send + Sync>, Box<dyn Error>>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let format = config.console_format();
    let with_ansi = config.console.with_ansi;

    let layer = formatter::build_formatter_from_config(config, format, with_ansi);

    Ok(layer)
}
