use std::fs;

use tracing_appender::{non_blocking, non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{layer::Layer as LayerTrait, registry::LookupSpan};

use crate::logging::{
    config::{LoggingConfig, RotationPolicy},
    formatter,
};

type DynLayer<S> = Box<dyn LayerTrait<S> + Send + Sync>;

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

    // File appender с rotation policy
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

    // Используем новый formatter с custom writer
    let boxed_layer =
        formatter::build_file_formatter_from_config(config, format, non_blocking_writer);

    Ok((boxed_layer, guard))
}
