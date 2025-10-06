use std::{fs, path::Path};

use tracing_appender::{non_blocking, non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{fmt, layer::Layer as LayerTrait, registry::LookupSpan};

use crate::logging::config::{LogFormat, LoggingConfig, RotationPolicy};

/// Тип-алиас для возвращаемого boxed слоя (чтобы уменьшить "type complexity").
type DynLayer<S> = Box<dyn LayerTrait<S> + Send + Sync>;

/// Возвращаем boxed Layer + guard.
pub fn layer<S>() -> (DynLayer<S>, WorkerGuard)
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let log_dir = Path::new("logs");
    let _ = fs::create_dir_all(log_dir);
    let file_appender = rolling::daily("logs", "output.log");
    let (non_blocking_writer, guard) = non_blocking(file_appender);

    let fmt_builder = fmt::format().compact();
    let layer = fmt::layer()
        .event_format(fmt_builder)
        .with_writer(non_blocking_writer);

    (Box::new(layer), guard)
}

pub fn layer_with_config<S>(
    config: &LoggingConfig
) -> Result<(DynLayer<S>, WorkerGuard), Box<dyn std::error::Error>>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let log_dir = &config.log_dir;
    let filename = &config.file.filename;
    let rotation = config.file_rotation();
    let format = config.file_format();

    // Создаём директорию
    fs::create_dir_all(log_dir)?;

    // Create file appender based on rotation policy
    let file_appender = match rotation {
        RotationPolicy::Daily => rolling::daily(log_dir, filename),
        RotationPolicy::Hourly => rolling::hourly(log_dir, filename),
        RotationPolicy::Never => rolling::never(log_dir, filename),
        RotationPolicy::Size { mb } => {
            eprintln!("Warning: Size-based rotation ({mb}MB) not yet implemented, using daily");
            rolling::daily(log_dir, filename)
        }
    };

    let (non_blocking_writer, guard) = non_blocking(file_appender);

    // В каждой ветке мы строим конкретный fmt::Layer и затем box'им его.
    let boxed_layer: DynLayer<S> = match format {
        LogFormat::Json => {
            // NOTE: .json() требует фичи "json" в tracing-subscriber
            let ev_fmt = fmt::format().json().with_current_span(true);
            let layer = fmt::layer()
                .event_format(ev_fmt)
                .with_writer(non_blocking_writer.clone())
                .with_ansi(false)
                .with_target(config.console.with_target)
                .with_thread_names(config.console.with_thread_ids)
                .with_thread_ids(config.console.with_thread_ids)
                .with_line_number(config.console.with_line_numbers);
            Box::new(layer)
        }
        LogFormat::Pretty => {
            let ev_fmt = fmt::format().pretty();
            let layer = fmt::layer()
                .event_format(ev_fmt)
                .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
                .with_writer(non_blocking_writer.clone())
                .with_ansi(false)
                .with_target(config.console.with_target)
                .with_thread_names(config.console.with_thread_ids)
                .with_thread_ids(config.console.with_thread_ids)
                .with_line_number(config.console.with_line_numbers);
            Box::new(layer)
        }
        LogFormat::Compact => {
            let ev_fmt = fmt::format().compact();
            let layer = fmt::layer()
                .event_format(ev_fmt)
                .with_writer(non_blocking_writer.clone())
                .with_ansi(false)
                .with_target(config.console.with_target)
                .with_thread_names(config.console.with_thread_ids)
                .with_thread_ids(config.console.with_thread_ids)
                .with_line_number(config.console.with_line_numbers);
            Box::new(layer)
        }
    };

    Ok((boxed_layer, guard))
}
