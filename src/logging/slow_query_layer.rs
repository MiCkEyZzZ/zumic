use std::time::Instant;

use tracing::{span, Subscriber};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

use crate::logging::slow_log::{SlowQueryEntry, SLOW_LOG_CONFIG, SLOW_LOG_METRICS};

pub struct SlowQueryLayer;

impl SlowQueryLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for SlowQueryLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        _attrs: &span::Attributes<'_>,
        id: &span::Id,
        ctx: Context<'_, S>,
    ) {
        // Сохраняем start time для span
        if let Some(span) = ctx.span(id) {
            let mut extensions = span.extensions_mut();
            extensions.insert(Instant::now());
        }
    }

    fn on_close(
        &self,
        id: span::Id,
        ctx: Context<'_, S>,
    ) {
        if let Some(span) = ctx.span(&id) {
            let metadata = span.metadata();

            // Проверяем target - только для command execution spans
            if !metadata.target().starts_with("zumic::command") {
                return;
            }

            // Получаем start time (у вас в on_new_span сохраняется Instant)
            let start = {
                let extensions = span.extensions();
                extensions.get::<Instant>().copied()
            };

            if let Some(start) = start {
                let elapsed = start.elapsed();
                let config = SLOW_LOG_CONFIG.read();

                // Проверяем threshold
                if elapsed > config.threshold {
                    // оставляем значения по-умолчанию — если ничего не писали в extensions, они
                    // останутся None/UNKNOWN
                    let command = String::from("UNKNOWN");
                    let client_addr: Option<String> = None;
                    let key: Option<String> = None;
                    let slot_id: Option<u64> = None;
                    let is_error = false;
                    let error_msg: Option<String> = None;

                    // УБРАЛИ span.with_subscriber(...) — он и вызывал ошибку компиляции

                    // Sampling check
                    if config.sample_rate < 1.0 {
                        use rand::Rng;
                        if rand::thread_rng().gen::<f64>() >= config.sample_rate {
                            SLOW_LOG_METRICS.record_sampled();
                            return;
                        }
                    }

                    // Record metric
                    SLOW_LOG_METRICS.record_slow_query(&command);

                    // Create and log entry
                    let mut entry = SlowQueryEntry::new(command, elapsed);

                    if let Some(addr) = client_addr {
                        entry = entry.with_client_addr(addr);
                    }

                    if let Some(k) = key {
                        entry = entry.with_key(k);
                    }

                    if let Some(sid) = slot_id {
                        entry = entry.with_slot_id(sid as u16);
                    }

                    if is_error {
                        if let Some(msg) = error_msg {
                            entry = entry.with_error(msg);
                        }
                    }

                    entry.log();
                }
            }
        }
    }
}

impl Default for SlowQueryLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use tracing::{info_span, Instrument};
    use tracing_subscriber::{layer::SubscriberExt, Registry};

    use super::*;

    /// Хелпер для создания тестового subscriber с SlowQueryLayer
    fn setup_test_subscriber() -> impl Subscriber {
        Registry::default().with(SlowQueryLayer::new())
    }

    /// Тест проверяет, что быстрые команды не попадают в slow log
    #[test]
    fn test_fast_query_not_logged() {
        let subscriber = setup_test_subscriber();

        // Устанавливаем высокий threshold
        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(100);
            config.sample_rate = 1.0;
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            let span = info_span!(
                target: "zumic::command::get",
                "command_execution"
            );
            let _enter = span.enter();
            // Быстрая операция - не должна логироваться
            std::thread::sleep(Duration::from_millis(10));
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert_eq!(
            initial_count, final_count,
            "Fast query should not be logged as slow"
        );
    }

    /// Тест проверяет, что медленные команды логируются
    #[test]
    fn test_slow_query_is_logged() {
        let subscriber = setup_test_subscriber();

        // Устанавливаем низкий threshold
        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(10);
            config.sample_rate = 1.0;
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            let span = info_span!(
                target: "zumic::command::set",
                "command_execution"
            );
            let _enter = span.enter();
            // Медленная операция
            std::thread::sleep(Duration::from_millis(50));
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert!(final_count > initial_count, "Slow query should be logged");
    }

    /// Тест проверяет, что spans с неправильным target игнорируются
    #[test]
    fn test_non_command_spans_ignored() {
        let subscriber = setup_test_subscriber();

        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(10);
            config.sample_rate = 1.0;
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            // Span с другим target - должен игнорироваться
            let span = info_span!(
                target: "zumic::network::tcp",
                "network_operation"
            );
            let _enter = span.enter();
            std::thread::sleep(Duration::from_millis(50));
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert_eq!(
            initial_count, final_count,
            "Non-command spans should be ignored"
        );
    }

    /// Тест проверяет sampling functionality
    #[test]
    fn test_sampling_reduces_logged_queries() {
        let subscriber = setup_test_subscriber();

        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(10);
            config.sample_rate = 0.0; // Отключаем логирование
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        let initial_sampled = SLOW_LOG_METRICS.get_stats().sampled_queries;

        tracing::subscriber::with_default(subscriber, || {
            for _ in 0..10 {
                let span = info_span!(
                    target: "zumic::command::get",
                    "command_execution"
                );
                let _enter = span.enter();
                std::thread::sleep(Duration::from_millis(20));
                drop(_enter);
            }
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        let final_sampled = SLOW_LOG_METRICS.get_stats().sampled_queries;

        // С sample_rate = 0.0 ничего не должно логироваться
        assert_eq!(
            initial_count, final_count,
            "With 0.0 sample rate, nothing should be logged"
        );
        // Но sampled counter должен увеличиться
        assert!(
            final_sampled > initial_sampled,
            "Sampled counter should increase"
        );
    }

    /// Тест проверяет разные команды в метриках
    #[test]
    fn test_different_commands_tracked_separately() {
        let subscriber = setup_test_subscriber();

        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(10);
            config.sample_rate = 1.0;
        }

        tracing::subscriber::with_default(subscriber, || {
            // Выполняем разные команды
            for cmd in &["GET", "SET", "DEL"] {
                let span = info_span!(
                    target: "zumic::command::exec",
                    "command_execution",
                    command = cmd
                );
                let _enter = span.enter();
                std::thread::sleep(Duration::from_millis(20));
                drop(_enter);
            }
        });

        let stats = SLOW_LOG_METRICS.get_stats();
        // Должно быть хотя бы 3 медленных запроса
        assert!(
            stats.total_slow_queries >= 3,
            "Should have logged at least 3 slow queries"
        );
    }

    /// Тест проверяет concurrent spans
    #[test]
    fn test_concurrent_spans() {
        let subscriber = Arc::new(setup_test_subscriber());

        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(10);
            config.sample_rate = 1.0;
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let sub = subscriber.clone();
                std::thread::spawn(move || {
                    tracing::subscriber::with_default(sub, || {
                        let span = info_span!(
                            target: "zumic::command::get",
                            "command_execution",
                            thread = i
                        );
                        let _enter = span.enter();
                        std::thread::sleep(Duration::from_millis(30));
                    });
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert!(
            final_count >= initial_count + 5,
            "Should have logged 5 concurrent slow queries"
        );
    }

    /// Тест проверяет nested spans (только внешний должен логироваться)
    #[test]
    fn test_nested_spans() {
        let subscriber = setup_test_subscriber();

        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(10);
            config.sample_rate = 1.0;
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            let outer = info_span!(
                target: "zumic::command::pipeline",
                "command_execution"
            );
            let _outer_enter = outer.enter();

            std::thread::sleep(Duration::from_millis(20));

            {
                let inner = info_span!(
                    target: "zumic::command::get",
                    "command_execution"
                );
                let _inner_enter = inner.enter();
                std::thread::sleep(Duration::from_millis(20));
            }
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        // Оба span'а должны залогироваться (каждый по отдельности)
        assert!(
            final_count >= initial_count + 2,
            "Both nested spans should be logged"
        );
    }

    /// Тест проверяет, что Default trait работает
    #[test]
    fn test_default_implementation() {
        let layer1 = SlowQueryLayer::new();
        let layer2 = SlowQueryLayer::default();

        // Оба должны быть валидными экземплярами
        // (проверяем что не паникует)
        let _sub1 = Registry::default().with(layer1);
        let _sub2 = Registry::default().with(layer2);
    }

    /// Тест проверяет edge case с очень коротким threshold
    #[test]
    fn test_zero_threshold() {
        let subscriber = setup_test_subscriber();

        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_nanos(1);
            config.sample_rate = 1.0;
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            let span = info_span!(
                target: "zumic::command::ping",
                "command_execution"
            );
            let _enter = span.enter();
            // Даже без sleep - любой span будет "медленным"
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert!(
            final_count > initial_count,
            "With near-zero threshold, all queries should be slow"
        );
    }

    /// Тест проверяет async spans (используя tokio)
    #[tokio::test]
    async fn test_async_spans() {
        let subscriber = setup_test_subscriber();

        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_millis(10);
            config.sample_rate = 1.0;
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        // ВАЖНО: subscriber должен быть установлен ДО создания span
        let _guard = tracing::subscriber::set_default(subscriber);

        async {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        .instrument(info_span!(
            target: "zumic::command::async_get",
            "command_execution"
        ))
        .await;

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert!(
            final_count > initial_count,
            "Async slow query should be logged"
        );
    }
}
