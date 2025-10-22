use std::io::{self, Stdout};

/// Для возврата используем trait-объект: Box<dyn
/// tracing_subscriber::layer::Layer<S> + Send + Sync>
use tracing_subscriber::layer::Layer as LayerTrait;
use tracing_subscriber::registry::LookupSpan;

use crate::logging::{
    config::{LogFormat, LoggingConfig},
    formats,
};

/// Build formatter на основе конфигурации.
/// Возвращаем boxed trait-объект для type erasure.
pub fn build_formatter_from_config<S>(
    config: &LoggingConfig,
    format: LogFormat,
    with_ansi: bool,
) -> Box<dyn LayerTrait<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let writer: fn() -> Stdout = io::stdout;

    match format {
        LogFormat::Json => formats::json::build_json_layer(config, writer, with_ansi),
        LogFormat::Pretty => formats::pretty::build_pretty_layer(config, writer, with_ansi),
        LogFormat::Compact => formats::compact::build_compact_layer(config, writer, with_ansi),
    }
}

/// Build formatter для файлового вывода (с custom writer)
pub fn build_file_formatter_from_config<S, W>(
    config: &LoggingConfig,
    format: LogFormat,
    writer: W,
) -> Box<dyn LayerTrait<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    match format {
        LogFormat::Json => formats::json::build_json_layer(config, writer, false),
        LogFormat::Pretty => formats::pretty::build_pretty_layer(config, writer, false),
        LogFormat::Compact => formats::compact::build_compact_layer(config, writer, false),
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::registry::Registry;

    use super::*;
    use crate::logging::config::{LogFormat, LoggingConfig};

    // Компиляторная проверка, что тип Send + Sync
    fn assert_send_sync<T: Send + Sync>() {}

    /// Тест проверят, что функции сборки форматтера не паникуют для всех
    /// форматов и возвращают валидный boxed layer.
    #[test]
    fn test_build_formatter_from_config_smoke() {
        // используем дефолтную конфигурацию (предварительно реализуйте Default для
        // LoggingConfig если нужно)
        let cfg = LoggingConfig::default();
        // Для конкретного Subscriber используем Registry
        let _json = build_formatter_from_config::<Registry>(&cfg, LogFormat::Json, true);
        let _pretty = build_formatter_from_config::<Registry>(&cfg, LogFormat::Pretty, true);
        let _compact = build_formatter_from_config::<Registry>(&cfg, LogFormat::Compact, true);
        // Если дошло до сюда — паники не произошло (smoke test)
    }

    /// Тест проверят, что возвращаемый boxed layer соответствует Send + Sync
    /// bound.
    #[test]
    fn test_layer_is_send_sync() {
        // проверка на уровне типов — если код компилируется, тест пройден
        type LayerBox = Box<dyn tracing_subscriber::layer::Layer<Registry> + Send + Sync>;
        assert_send_sync::<LayerBox>();
    }
}
