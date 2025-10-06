use tracing_subscriber::{
    fmt::{self as other_fmt, format::FmtSpan},
    layer::Layer as LayerTrait,
    registry::LookupSpan,
};

use crate::logging::config::LoggingConfig;

/// Создаёт Pretty formatter layer (для development)
pub fn build_pretty_layer<S, W>(
    config: &LoggingConfig,
    writer: W,
    with_ansi: bool,
) -> Box<dyn LayerTrait<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    let pretty_fmt = other_fmt::format().pretty();

    let layer = other_fmt::layer()
        .event_format(pretty_fmt)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(writer)
        .with_ansi(with_ansi)
        .with_target(config.console.with_target)
        .with_thread_names(true)
        .with_thread_ids(config.console.with_thread_ids)
        .with_line_number(config.console.with_line_numbers);

    Box::new(layer)
}
