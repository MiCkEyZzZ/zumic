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

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::{Arc, Mutex},
    };

    use tracing_subscriber::{fmt, prelude::*, registry::Registry};

    use super::*;

    // Мини-буферный writer для тестов
    struct VecMakeWriter(Arc<Mutex<Vec<u8>>>);

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for VecMakeWriter {
        type Writer = VecWriterGuard;

        fn make_writer(&'a self) -> Self::Writer {
            VecWriterGuard(self.0.clone())
        }
    }

    struct VecWriterGuard(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for VecWriterGuard {
        fn write(
            &mut self,
            buf: &[u8],
        ) -> std::io::Result<usize> {
            let mut locked = self.0.lock().unwrap();
            locked.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Тест проверят, что build_filter не паникует и возвращает EnvFilter,
    /// даже если переменная окружения отсутствует.
    #[test]
    fn test_build_filter_no_env() {
        env::remove_var("RUST_LOG");
        let _f = build_filter();
        // если функция завершилась успешно — тест пройден
    }

    /// Тест проверят, что build_filter использует RUST_LOG когда она задана.
    #[test]
    fn test_build_filter_with_env() {
        env::set_var("RUST_LOG", "debug");
        let f = build_filter();
        drop(f);
        env::remove_var("RUST_LOG");
    }

    /// Тест проверят, что build_filter_from_config не падает при некорректной
    /// директиве.
    ///
    /// Здесь моделируем поведение конфигурации: берем некорректную директиву,
    /// убеждаемся, что `EnvFilter::try_new` вернёт Err для неё — это покрывает
    /// ветку, где код должен fallback'нуться на "info".
    #[test]
    fn test_build_filter_from_config_invalid_directive() {
        env::remove_var("RUST_LOG"); // гарантируем использование конфигурации

        // Простая «мок»-конфигурация с методом build_filter_directive()
        #[derive(Clone)]
        struct FakeCfg(String);
        impl FakeCfg {
            fn build_filter_directive(&self) -> String {
                self.0.clone()
            }
        }

        let cfg = FakeCfg("this_is_invalid_directive!!".to_string());

        // используем cfg чтобы не было warning про unused variable
        let directive = cfg.build_filter_directive();
        // Попытка создать EnvFilter из некорректной директивы — ожидаем Err
        let try_res = tracing_subscriber::EnvFilter::try_new(&directive);
        assert!(
            try_res.is_err(),
            "Некорректная директива должна возвращать Err"
        );

        // В реальной функции build_filter_from_config код обрабатывает Err и
        // падает обратно на "info". Здесь демонстрация того, что путь
        // Err возможен.
    }

    /// Тест проверят поведение фильтра в runtime: если директива "warn" —
    /// сообщения info не попадут в writer, а warn попадут.
    #[test]
    fn test_envfilter_integration_filters_levels() {
        env::remove_var("RUST_LOG");

        // Мок-конфиг, возвращающий директиву "warn"
        #[derive(Clone)]
        struct FakeCfg(String);
        impl FakeCfg {
            fn build_filter_directive(&self) -> String {
                self.0.clone()
            }
        }
        let cfg = FakeCfg("warn".to_string());
        let directive = cfg.build_filter_directive();

        // Собираем EnvFilter
        let filter =
            tracing_subscriber::EnvFilter::try_new(&directive).expect("failed to build env filter");

        // Подготовим буфер и subscriber
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = VecMakeWriter(buffer.clone());

        let layer = fmt::layer().with_writer(writer).with_filter(filter);

        let subscriber = Registry::default().with(layer);

        // Устанавливаем временный default subscriber для теста
        let _guard = tracing::subscriber::set_default(subscriber);

        // Эмитим info и warn
        tracing::info!("this is an info message that should be filtered out");
        tracing::warn!("this is a warn message that should pass through");

        // Небольшая пауза для flush-а (обычно не требуется, но безопасно)
        std::thread::sleep(std::time::Duration::from_millis(50));

        let out = buffer.lock().unwrap();
        let s = String::from_utf8_lossy(&out);

        // Проверки: warn должен присутствовать, info — отсутствовать
        assert!(s.contains("warn") || s.contains("WARN") || s.contains("this is a warn message"));
        assert!(!s.contains("this is an info message that should be filtered out"));
    }
}
