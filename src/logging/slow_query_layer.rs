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

            if !metadata.target().starts_with("zumic::command") {
                return;
            }

            let start = {
                let extensions = span.extensions();
                extensions.get::<Instant>().copied()
            };

            if let Some(start) = start {
                let elapsed = start.elapsed();
                let config = SLOW_LOG_CONFIG.read();

                if elapsed > config.threshold {
                    let command = String::from("UNKNOWN");
                    let client_addr: Option<String> = None;
                    let key: Option<String> = None;
                    let slot_id: Option<u64> = None;
                    let is_error = false;
                    let error_msg: Option<String> = None;

                    if config.sample_rate < 1.0 {
                        use rand::Rng;
                        if rand::thread_rng().gen::<f64>() >= config.sample_rate {
                            SLOW_LOG_METRICS.record_sampled();
                            return;
                        }
                    }

                    SLOW_LOG_METRICS.record_slow_query(&command);

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
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use tracing::{info_span, Instrument};
    use tracing_subscriber::{layer::SubscriberExt, Registry};

    use super::*;

    static TEST_LOCK: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

    fn setup_test_subscriber() -> impl Subscriber {
        Registry::default().with(SlowQueryLayer::new())
    }

    /// Вспомогательная функция: захватывает lock и настраивает конфиг
    /// Использует unwrap_or_else для обработки PoisonError
    fn setup_test(
        threshold_ms: u64,
        sample_rate: f64,
    ) -> std::sync::MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let mut config = SLOW_LOG_CONFIG.write();
        config.threshold = Duration::from_millis(threshold_ms);
        config.sample_rate = sample_rate;
        drop(config);

        guard
    }

    #[test]
    fn test_fast_query_not_logged() {
        let _guard = setup_test(100, 1.0);
        let subscriber = setup_test_subscriber();

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            let span = info_span!(
                target: "zumic::command::get",
                "command_execution"
            );
            let _enter = span.enter();
            std::thread::sleep(Duration::from_millis(10));
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert_eq!(
            initial_count, final_count,
            "Fast query should not be logged as slow"
        );
    }

    #[test]
    fn test_slow_query_is_logged() {
        let _guard = setup_test(10, 1.0);
        let subscriber = setup_test_subscriber();

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            let span = info_span!(
                target: "zumic::command::set",
                "command_execution"
            );
            let _enter = span.enter();
            std::thread::sleep(Duration::from_millis(50));
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert!(final_count > initial_count, "Slow query should be logged");
    }

    #[test]
    fn test_non_command_spans_ignored() {
        let _guard = setup_test(10, 1.0);
        let subscriber = setup_test_subscriber();

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
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

    #[test]
    fn test_sampling_reduces_logged_queries() {
        let _guard = setup_test(10, 0.0);
        let subscriber = setup_test_subscriber();

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

        assert_eq!(
            initial_count, final_count,
            "With 0.0 sample rate, nothing should be logged"
        );
        assert!(
            final_sampled > initial_sampled,
            "Sampled counter should increase"
        );
    }

    #[test]
    fn test_different_commands_tracked_separately() {
        let _guard = setup_test(10, 1.0);
        let subscriber = setup_test_subscriber();

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
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
        assert!(
            stats.total_slow_queries >= initial_count + 3,
            "Should have logged at least 3 slow queries"
        );
    }

    #[test]
    fn test_concurrent_spans() {
        let _guard = setup_test(10, 1.0);
        let subscriber = Arc::new(setup_test_subscriber());

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

    #[test]
    fn test_nested_spans() {
        let _guard = setup_test(10, 1.0);
        let subscriber = setup_test_subscriber();

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
        assert!(
            final_count >= initial_count + 2,
            "Both nested spans should be logged"
        );
    }

    #[test]
    fn test_zero_threshold() {
        let _guard = setup_test(0, 1.0);
        let subscriber = setup_test_subscriber();

        // Используем from_nanos для очень маленького threshold
        {
            let mut config = SLOW_LOG_CONFIG.write();
            config.threshold = Duration::from_nanos(1);
        }

        let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

        tracing::subscriber::with_default(subscriber, || {
            let span = info_span!(
                target: "zumic::command::ping",
                "command_execution"
            );
            let _enter = span.enter();
        });

        let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;
        assert!(
            final_count > initial_count,
            "With near-zero threshold, all queries should be slow"
        );
    }

    #[tokio::test]
    async fn test_async_spans() {
        // Запускаем async работу в блокирующем контексте с удержанием guard
        let result = tokio::task::spawn_blocking(|| {
            let _guard = setup_test(10, 1.0);
            let subscriber = setup_test_subscriber();

            let initial_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

            // Создаём tokio runtime внутри блокирующей задачи
            let rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                let _tracing_guard = tracing::subscriber::set_default(subscriber);

                async {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
                .instrument(info_span!(
                    target: "zumic::command::async_get",
                    "command_execution"
                ))
                .await;
            });

            let final_count = SLOW_LOG_METRICS.get_stats().total_slow_queries;

            (initial_count, final_count)
        })
        .await
        .unwrap();

        assert!(result.1 > result.0, "Async slow query should be logged");
    }
}
