use std::fs;
use std::path::Path;
use tracing_appender::{non_blocking, rolling::daily};
use tracing_subscriber::{fmt, Layer};

pub fn layer<S>() -> (impl Layer<S>, tracing_appender::non_blocking::WorkerGuard)
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    // Создаем директорию, если она не существует
    let log_dir = Path::new("logs");
    let _ = fs::create_dir_all(log_dir);

    // Настроим файл для записи логов
    let file_appender = daily("logs", "output.log");
    let (non_blocking_writer, guard) = non_blocking(file_appender);

    // Форматирование логов
    let layer = fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking_writer);

    (layer, guard)
}
