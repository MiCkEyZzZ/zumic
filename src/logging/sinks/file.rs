use std::{fs, sync::Arc};

use tracing_appender::{non_blocking, non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{layer::Layer as LayerTrait, registry::LookupSpan};

use crate::logging::{
    config::{LoggingConfig, RotationPolicy},
    formatter,
    sinks::rotation::{
        apply_retention_policy, compress_log_file, RetentionPolicy, RotationMetrics,
    },
};

type DynLayer<S> = Box<dyn LayerTrait<S> + Send + Sync>;

// Глобальные метрики ротации (для мониторинга).
lazy_static::lazy_static! {
    pub static ref ROTATION_METRICS: Arc<RotationMetrics> = Arc::new(RotationMetrics::new());
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

    // File appender с rotation policy
    let file_appender = match rotation {
        RotationPolicy::Daily => rolling::daily(log_dir, filename),
        RotationPolicy::Hourly => rolling::hourly(log_dir, filename),
        RotationPolicy::Never => rolling::never(log_dir, filename),
        RotationPolicy::Size { mb } => {
            tracing::info!(size_mb = mb, "Using size-based rotation");

            // TODO: Полная реализация size-based через custom appender
            // Пока используем daily с предупреждением
            eprintln!(
                "Warning: Size-based rotation ({}MB) partially implemented, using daily rotation.\n\
                 Background cleanup will run periodically.",
                mb
            );

            rolling::daily(log_dir, filename)
        }
    };

    let (non_blocking_writer, guard) = non_blocking(file_appender);

    // Используем formatter с custom writer
    let boxed_layer =
        formatter::build_file_formatter_from_config(config, format, non_blocking_writer);

    // Запускаем background задачи для rotation
    start_rotation_tasks(config)?;

    Ok((boxed_layer, guard))
}

/// Получение текущих метрик ротации.
pub fn get_rotation_stats() -> super::rotation::RotationStats {
    ROTATION_METRICS.get_stats()
}

/// Запускает background задачи для rotation.
fn start_rotation_tasks(config: &LoggingConfig) -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = config.log_dir.clone();
    let compress_old = config.file.compress_old;
    let retention_days = config.file.retention_days.unwrap_or(config.retention_days);

    // Background задача для compression и retention.
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));

        loop {
            interval.tick().await;

            // Compression старых файлов
            if compress_old {
                if let Err(e) = compress_old_logs(&log_dir) {
                    tracing::error!(error = %e, "Failed to compress old logs");
                }
            }

            // Retention policy
            let policy = RetentionPolicy {
                max_age_days: Some(retention_days),
                max_files: None,
                max_total_size_bytes: None,
            };

            if let Err(e) = apply_retention_policy(&log_dir, &policy, &ROTATION_METRICS) {
                tracing::error!(error = %e, "Failed to apply retention policy");
            }

            let stats = ROTATION_METRICS.get_stats();
            tracing::info!(
                rotation_count = stats.rotation_count,
                compressed_count = stats.compressed_count,
                deleted_count = stats.deleted_count,
                deleted_mb = stats.deleted_bytes / 1024 / 1024,
                "Rotation statistics"
            );
        }
    });

    Ok(())
}

/// Сжимает старые log файлы (не .gz).
fn compress_old_logs(log_dir: &std::path::Path) -> std::io::Result<()> {
    let entries = fs::read_dir(log_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            // Проверяем расширение
            if let Some(ext) = path.extension() {
                if ext == "log" {
                    // Проверяем возраст (> 1 день)
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(duration) =
                                std::time::SystemTime::now().duration_since(modified)
                            {
                                if duration.as_secs() > 86400 {
                                    // Старше 1 дня
                                    if let Err(e) = compress_log_file(&path, &ROTATION_METRICS) {
                                        tracing::warn!(
                                            path = %path.display(),
                                            error = %e,
                                            "Failed to compress log file"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
