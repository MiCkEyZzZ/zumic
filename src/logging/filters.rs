use tracing_subscriber::EnvFilter;

use crate::logging::config::LoggingConfig;

#[allow(dead_code)]
pub fn build_filter() -> EnvFilter {
    EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap()
}

#[allow(dead_code)]
pub fn build_filter_from_config(config: &LoggingConfig) -> EnvFilter {
    // Директива, полученная из конфига (например "zumic=info,...")
    let directive = config.build_filter_directive();

    // Если RUST_LOG (или другой env filter) задан — используем его.
    // Если переменная окружения отсутствует — try_from_default_env() вернёт Err.
    match EnvFilter::try_from_default_env() {
        Ok(env_filter) => env_filter,
        Err(_) => {
            // Попытаемся собрать EnvFilter из нашей конфигурации
            match EnvFilter::try_new(&directive) {
                Ok(filter) => filter,
                Err(e) => {
                    // Возможно, конфигурация содержит некорректную директиву —
                    // на этот случай печатаем понятное сообщение и падаем
                    eprintln!("Invalid log filter directive from config ('{}'): {}; falling back to 'info'", directive, e);
                    EnvFilter::new("info")
                }
            }
        }
    }
}
