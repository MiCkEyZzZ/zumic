use std::io::{self, Stdout};

/// Для возврата используем trait-объект: Box<dyn
/// tracing_subscriber::layer::Layer<S> + Send + Sync>
use tracing_subscriber::layer::Layer as LayerTrait;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    registry::LookupSpan,
};

use crate::logging::config::{LogFormat, LoggingConfig};

#[allow(dead_code)]
pub fn build_formatter<S>() -> impl LayerTrait<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_thread_names(true)
        .with_thread_ids(true)
}

/// Build formatter на основе конфигурации.
/// Возвращаем boxed trait-объект, чтобы стереть конкретный тип формата
/// (json/pretty/compact).
pub fn build_formatter_from_config<S>(
    config: &LoggingConfig,
    format: LogFormat,
    with_ansi: bool,
) -> Box<dyn LayerTrait<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    // Явно указываем writer как fn() -> Stdout
    let writer: fn() -> Stdout = io::stdout;

    match format {
        LogFormat::Json => {
            let json_fmt = fmt::format().json().with_current_span(true);
            let layer = fmt::layer()
                .event_format(json_fmt)
                .with_writer(writer)
                .with_ansi(with_ansi)
                .with_target(config.console.with_target)
                .with_thread_names(config.console.with_thread_ids)
                .with_thread_ids(config.console.with_thread_ids)
                .with_line_number(config.console.with_line_numbers);
            Box::new(layer)
        }
        LogFormat::Pretty => {
            let pretty_fmt = fmt::format().pretty();
            let layer = fmt::layer()
                .event_format(pretty_fmt)
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(writer)
                .with_ansi(with_ansi)
                .with_target(config.console.with_target)
                .with_thread_names(config.console.with_thread_ids)
                .with_thread_ids(config.console.with_thread_ids)
                .with_line_number(config.console.with_line_numbers);
            Box::new(layer)
        }
        LogFormat::Compact => {
            let compact_fmt = fmt::format().compact();
            let layer = fmt::layer()
                .event_format(compact_fmt)
                .with_writer(writer)
                .with_ansi(with_ansi)
                .with_target(config.console.with_target)
                .with_thread_names(config.console.with_thread_ids)
                .with_thread_ids(config.console.with_thread_ids)
                .with_line_number(config.console.with_line_numbers);
            Box::new(layer)
        }
    }
}
