use tracing_subscriber::{fmt, layer::Layer as LayerTrait, registry::LookupSpan};

use crate::logging::config::LoggingConfig;

/// Создаёт Compact formatter layer (для containers с ограниченным местом)
pub fn build_compact_layer<S, W>(
    config: &LoggingConfig,
    writer: W,
    with_ansi: bool,
) -> Box<dyn LayerTrait<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    let compact_fmt = fmt::format().compact();

    let layer = fmt::layer()
        .event_format(compact_fmt)
        .with_writer(writer)
        .with_ansi(with_ansi)
        .with_target(config.console.with_target)
        .with_thread_names(false) // Compact - минимум информации
        .with_thread_ids(false)
        .with_line_number(false);

    Box::new(layer)
}
