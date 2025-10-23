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

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Write},
        sync::{Arc, Mutex},
    };

    use tracing::info;
    use tracing_subscriber::{prelude::*, registry::Registry};

    use super::*;

    /// Простая обёртка writer-а, которую можно передавать в `with_writer`.
    /// Тест проверят, что сообщения логера попадают в provided writer.
    struct BufferWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for BufferWriter {
        fn write(
            &mut self,
            buf: &[u8],
        ) -> io::Result<usize> {
            let mut guard = self.0.lock().unwrap();
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// MakeWriter closure возвращающий `BufferWriter`.
    fn make_buffer_writer(
        buf: Arc<Mutex<Vec<u8>>>
    ) -> impl for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static {
        move || BufferWriter(buf.clone())
    }

    /// Тест проверят, что build_compact_layer не паникует и что сообщение
    /// попадает в writer.
    #[test]
    fn test_build_compact_layer_smoke_and_output() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let cfg = LoggingConfig::default();

        // Создаём layer c нашим writer-ом (ANSI выключим для детерминированности).
        let layer = build_compact_layer(&cfg, make_buffer_writer(buf.clone()), false);

        // Регистрируем и логируем
        let subscriber = Registry::default().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            info!("compact-test-message");
        });

        // Проверяем, что что-то записалось в буфер (строка с нашим текстом)
        let content = String::from_utf8(buf.lock().unwrap().clone()).unwrap_or_default();
        assert!(
            content.contains("compact-test-message"),
            "expected message in output; got: {content}",
        );
    }

    /// Тест проверят, что включение ANSI не ломает создание слоя и логирование
    /// (smoke).
    #[test]
    fn test_build_compact_layer_with_ansi_smoke() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let cfg = LoggingConfig::default();
        // compact layer обычно минимален — проверим с ANSI=true
        let layer = build_compact_layer(&cfg, make_buffer_writer(buf.clone()), true);

        let subscriber = Registry::default().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            info!("compact-ansi-test");
        });

        // Достаточно убедиться, что writer получил данные (не важно, с ANSI или без)
        let content = String::from_utf8(buf.lock().unwrap().clone()).unwrap_or_default();
        assert!(
            !content.is_empty(),
            "expected some output when ANSI enabled"
        );
    }
}
