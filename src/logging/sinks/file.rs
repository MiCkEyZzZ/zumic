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
                "Warning: Size-based rotation ({mb}MB) partially implemented, using daily rotation.\n\
                 Background cleanup will run periodically."
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

    // Если нет текущего tokio runtime — пропускаем запуск фоновой таски (удобно для
    // unit-тестов).
    if tokio::runtime::Handle::try_current().is_err() {
        tracing::debug!("No tokio runtime found, skipping rotation background task (test mode).");
        return Ok(());
    }

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

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use filetime::{set_file_mtime, FileTime};
    use tempfile::tempdir;
    use tracing::info;
    use tracing_subscriber::{prelude::*, registry::Registry};

    use super::*;
    use crate::logging::config::FileConfig;

    /// Тест проверят, что layer_with_config создаёт директорию, возвращает Ok
    /// и что layer можно зарегистрировать и отправить сообщение (smoke).
    #[test]
    fn test_layer_with_config_creates_dir_and_returns_guard() {
        // tmp dir
        let dir = tempdir().unwrap();

        // Конфигурация сразу при инициализации
        let cfg = LoggingConfig {
            log_dir: dir.path().to_path_buf(),
            file: FileConfig {
                filename: "test_zumic.log".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        // Убедимся, что папка ещё не содержит логов
        assert!(dir.path().read_dir().unwrap().next().is_none());

        // Создаём layer и guard
        let (boxed_layer, guard) =
            layer_with_config::<Registry>(&cfg).expect("layer_with_config failed");

        // Регистрация и логирование (не должно паниковать)
        let subscriber = Registry::default().with(boxed_layer);
        tracing::subscriber::with_default(subscriber, || {
            info!("hello from test_layer_with_config_creates_dir_and_returns_guard");
        });

        // guard присутствует (не panic при drop)
        drop(guard);

        // Проверим, что директория содержит хотя бы один файл-логгер
        let found_any = std::fs::read_dir(cfg.log_dir).unwrap().any(|e| {
            let p = e.unwrap().path();
            p.is_file()
        });
        assert!(
            found_any,
            "expected at least one file in log_dir after creating layer"
        );
    }

    /// Тест проверят, что свежие .log файлы не сжимаются.
    #[test]
    fn test_compress_old_logs_skips_recent_files() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("recent.log");
        std::fs::write(&log_path, b"recent").unwrap();

        // Установим mtime на сейчас
        let now = FileTime::from_system_time(SystemTime::now());
        set_file_mtime(&log_path, now).unwrap();

        // Вызовем compress_old_logs — должен пропустить свежий файл
        compress_old_logs(dir.path()).expect("compress_old_logs failed");

        // убедимся, что оригинал остался и .gz не появился
        assert!(log_path.exists());
        let gz1 = dir.path().join("recent.log.gz");
        let gz2 = dir.path().join("recent.gz");
        assert!(
            !gz1.exists() && !gz2.exists(),
            "recent file should not be compressed"
        );
    }

    /// Тест проверят, что старые .log файлы пытаются сжаться (ожидаем .gz).
    #[test]
    fn test_compress_old_logs_compresses_old_files() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("old.log");
        std::fs::write(&log_path, b"very old").unwrap();

        // Установим mtime на 3 дня назад
        let old_time =
            FileTime::from_system_time(SystemTime::now() - Duration::from_secs(3 * 24 * 3600));
        set_file_mtime(&log_path, old_time).unwrap();

        // Вызовем compress_old_logs — реализация внутри может либо создать old.log.gz
        // либо old.gz (в зависимости от impl). Проверим наличие хотя бы одного из них.
        compress_old_logs(dir.path()).expect("compress_old_logs failed");

        let gz1 = dir.path().join("old.log.gz");
        let gz2 = dir.path().join("old.gz");
        assert!(
            gz1.exists() || gz2.exists(),
            "expected compressed file (old.log.gz or old.gz) to be present"
        );
    }

    /// Тест проверят, что start_rotation_tasks не падает когда есть tokio
    /// runtime. Мы просто проверяем, что функция вернёт Ok (фоновые таски
    /// будут спавнены).
    #[tokio::test(flavor = "current_thread")]
    async fn test_start_rotation_tasks_runs_when_runtime_exists() {
        let dir = tempdir().unwrap();

        // Конфигурация инициализируется сразу: задаём log_dir и нужные поля file.*
        let cfg = LoggingConfig {
            log_dir: dir.path().to_path_buf(),
            file: FileConfig {
                compress_old: false,
                retention_days: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };

        // Should return Ok even if runtime exists (we spawn a background task).
        let res = super::start_rotation_tasks(&cfg);
        assert!(
            res.is_ok(),
            "start_rotation_tasks should return Ok when runtime present"
        );

        // Let the spawned task spin up a tiny bit (it ticks every hour, but spawn
        // should not block)
        tokio::task::yield_now().await;
    }
}
