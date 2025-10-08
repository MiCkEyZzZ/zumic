use tracing_subscriber::{
    filter::FilterFn,
    fmt::{self},
    layer::Layer as LayerTrait,
    registry::LookupSpan,
};

use crate::logging::config::LoggingConfig;

/// JSON formatter с кастомными полями (для future use).
#[allow(dead_code)]
#[derive(Debug)]
pub struct JsonFormatter {
    instance_id: Option<String>,
    version: String,
    environment: Option<String>,
    hostname: Option<String>,
}

impl JsonFormatter {
    pub fn new(config: &LoggingConfig) -> Self {
        Self {
            instance_id: config.custom_fields.instance_id.clone(),
            version: config
                .custom_fields
                .version
                .clone()
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
            environment: config.custom_fields.environment.clone(),
            hostname: config.custom_fields.hostname.clone(),
        }
    }
}

/// Создаёт JSON formatter layer.
pub fn build_json_layer<S, W>(
    config: &LoggingConfig,
    writer: W,
    with_ansi: bool,
) -> Box<dyn LayerTrait<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    let json_fmt = fmt::format()
        .json()
        .with_current_span(config.span.include_name)
        .with_span_list(config.span.include_full_list);

    // Функция-конструктор для базового слоя (чтобы избежать дублирования)
    let base_layer = || {
        fmt::layer()
            .event_format(json_fmt.clone())
            .with_writer(writer)
            .with_ansi(with_ansi)
            .with_target(config.console.with_target)
            .with_thread_names(true)
            .with_thread_ids(config.console.with_thread_ids)
            .with_line_number(config.console.with_line_numbers)
    };

    // Создаём trait-объект сразу, чтобы в обоих случаях возвращаем один и тот же
    // тип
    let layer: Box<dyn LayerTrait<S> + Send + Sync> = if config.custom_fields.instance_id.is_some()
    {
        // Если instance_id задан — применим лёгкий фильтр (пока-заглушка).
        // Для передачи замыкалки в with_filter используем FilterFn::new(...)
        let filtered = base_layer().with_filter(FilterFn::new(move |_metadata| {
            // TODO: Интегрировать instance_id в JSON output
            true
        }));
        Box::new(filtered)
    } else {
        Box::new(base_layer())
    };

    layer
}
