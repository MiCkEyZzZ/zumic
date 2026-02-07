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

#[cfg(test)]
mod tests {
    use tracing::info;
    use tracing_subscriber::{prelude::*, registry::Registry};

    use super::*;
    use crate::logging::config::ConsoleConfig;

    /// Тест проверят, что базовый console layer (layer) можно зарегистрировать
    /// в tracing::Registry и что вызов логирования не приводит к панике.
    #[test]
    fn test_layer_registers_and_logs_without_panic() {
        let layer = layer::<Registry>();
        let subscriber = Registry::default().with(layer);

        // Используем with_default чтобы не трогать глобальный subscriber навсегда.
        tracing::subscriber::with_default(subscriber, || {
            // Вызов логирования — не должен паниковать.
            info!("test message from test_layer_registers_and_logs_without_panic");
        });
    }

    /// Тест проверят, что layer_with_config возвращает Ok и что полученный
    /// layer можно зарегистрировать и использовать.
    #[test]
    fn test_layer_with_config_returns_layer_and_works() {
        // Настраиваем минимальную конфигурацию сразу через struct literal
        let cfg = LoggingConfig {
            console_enabled: true,
            console: ConsoleConfig {
                with_ansi: false, // меняем опцию, чтобы покрыть ветку
                ..Default::default()
            },
            ..Default::default()
        };

        // Создаём layer
        let boxed_layer_result = layer_with_config::<Registry>(&cfg);
        assert!(
            boxed_layer_result.is_ok(),
            "layer_with_config должен вернуть Ok"
        );

        let boxed_layer = boxed_layer_result.unwrap();

        // В Registry можно добавить Box<dyn Layer>
        let subscriber = Registry::default().with(boxed_layer);

        tracing::subscriber::with_default(subscriber, || {
            info!("test message from test_layer_with_config_returns_layer_and_works");
        });
    }

    /// Тест проверят, что layer_with_config не паникует при разных комбинациях
    /// console.format / with_ansi (простейшая smoke-проверка).
    #[test]
    fn test_layer_with_config_various_flags() {
        let mut cfg = LoggingConfig::default();

        for ansi in [true, false] {
            cfg.console.with_ansi = ansi;
            cfg.console_enabled = true;

            let l = layer_with_config::<Registry>(&cfg);
            assert!(
                l.is_ok(),
                "layer_with_config должен вернуть Ok при with_ansi={ansi}"
            );

            // не регистрируем subscriber — достаточно убедиться, что построение
            // layer не падает
        }
    }
}
