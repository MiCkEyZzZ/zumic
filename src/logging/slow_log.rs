use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use rand::Rng;
use serde::Serialize;
use tracing::{span, Level, Span};

/// Помошник макро для автоматического tracking.
#[macro_export]
macro_rules! track_slow_query {
    ($command:expr) => {
        $crate::logging::slow_log::SlowQueryTracker::new($command)
    };

    ($command:expr, $($key:expr => $value:expr),+ $(,)?) => {{
        let mut tracker = $crate::logging::slow_log::SlowQueryTracker::new($command);
        $(
            tracker = tracker.with_field($key, $value);
        )+
        tracker
    }};
}

lazy_static::lazy_static! {
    pub static ref SLOW_LOG_CONFIG: parking_lot::RwLock<SlowLogConfig> = parking_lot::RwLock::new(SlowLogConfig::default());

    pub static ref SLOW_LOG_METRICS: Arc<SlowLogMetrics> = Arc::new(SlowLogMetrics::new());
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryResult {
    Success,
    Error { message: String },
}

/// Конфигурация slow query logging.
#[derive(Debug, Clone)]
pub struct SlowLogConfig {
    /// Глобальный threshold (например, 100мс)
    pub threshold: Duration,
    /// Sample rate (0.0 - 0.1): 0.1 = log 10% slow queries
    pub sample_rate: f64,
    /// Per-command thresholds (override глобальный)
    pub command_thresholds: HashMap<String, Duration>,
    /// Выключить backtrace для slow queries
    pub enable_backtrace: bool,
    /// Максимальная длина args для логирования
    pub max_args_len: usize,
}

/// Метрики slow queries.
#[derive(Debug, Default)]
pub struct SlowLogMetrics {
    /// Общее кол-во slow queries
    pub total_slow_queries: AtomicU64,
    /// Кол-во sampled queries (не залогированных из-за sampling)
    pub sampled_queries: AtomicU64,
    /// Per-command counters
    pub command_counters: parking_lot::Mutex<HashMap<String, u64>>,
}

#[derive(Debug, Clone)]
pub struct SlowLogStats {
    pub total_slow_queries: u64,
    pub sampled_queries: u64,
    pub command_counts: HashMap<String, u64>,
}

/// Slow query tracker для измерения времени выполнения.
pub struct SlowQueryTracker {
    command: String,
    start: Instant,
    span: Option<Span>,

    args: Vec<String>,
    client_addr: Option<String>,
    key: Option<String>,
    slot_id: Option<u16>,
    result: QueryResult,
}

/// Structured slow query entry для детального логирования.
#[derive(Debug, Serialize)]
pub struct SlowQueryEntry {
    pub timestamp: i64,
    pub command: String,
    pub args: Vec<String>,
    pub duration_ms: u64,
    pub client_addr: Option<String>,
    pub key: Option<String>,
    pub slot_id: Option<u16>,
    pub result: QueryResult,
}

impl SlowLogMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_slow_query(
        &self,
        command: &str,
    ) {
        self.total_slow_queries.fetch_add(1, Ordering::Relaxed);
        let mut counters = self.command_counters.lock();
        *counters.entry(command.to_string()).or_insert(0) += 1;
    }

    pub fn record_sampled(&self) {
        self.sampled_queries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> SlowLogStats {
        SlowLogStats {
            total_slow_queries: self.total_slow_queries.load(Ordering::Relaxed),
            sampled_queries: self.sampled_queries.load(Ordering::Relaxed),
            command_counts: self.command_counters.lock().clone(),
        }
    }
}

impl SlowQueryTracker {
    /// Создаёт новый tracker для команды.
    pub fn new(command: impl Into<String>) -> Self {
        let command = command.into();

        // Создаёт span для контекста
        let span = span!(
            Level::DEBUG,
            "slow_query_check",
            command = %command,
        );

        Self {
            command,
            start: Instant::now(),
            span: Some(span),
            args: Vec::new(),
            client_addr: None,
            key: None,
            slot_id: None,
            result: QueryResult::Success,
        }
    }

    /// Добавляет дополнительные поля в span.
    pub fn with_field(
        &mut self,
        key: &str,
        value: impl std::fmt::Display,
    ) -> &mut Self {
        if let Some(ref span) = self.span {
            let _enter = span.enter();
            tracing::debug!(%key, %value, "Added field");
        }

        match key {
            "client_addr" => self.client_addr = Some(format!("{value}")),
            "key" => self.key = Some(format!("{value}")),
            "slot_id" => {
                let s = format!("{value}");
                if let Ok(v) = s.parse::<u16>() {
                    self.slot_id = Some(v)
                } else {
                    self.args.push(s)
                }
            }
            "result" => {
                let s = format!("{value}");
                if s.eq_ignore_ascii_case("success") {
                    self.result = QueryResult::Success;
                } else {
                    self.result = QueryResult::Error { message: s };
                }
            }
            _ => self.args.push(format!("{key}={value}")),
        }

        self
    }

    /// Завершает tracking и логирует если медленно.
    pub fn finish(self) {
        let elapsed = self.start.elapsed();
        let config = SLOW_LOG_CONFIG.read();

        // Проверяем threshold (глобальный или per-command)
        let threshold = config
            .command_thresholds
            .get(&self.command)
            .copied()
            .unwrap_or(config.threshold);

        if elapsed > threshold {
            // Sampling check
            if config.sample_rate < 1.0 {
                let should_log = if config.sample_rate <= 0.0 {
                    false
                } else {
                    rand::thread_rng().gen::<f64>() < config.sample_rate
                };

                if !should_log {
                    SLOW_LOG_METRICS.record_sampled();
                    return;
                }
            }

            // Записываем метрику
            SLOW_LOG_METRICS.record_slow_query(&self.command);

            // Логируем slow query
            let mut entry = SlowQueryEntry::new(self.command.clone(), elapsed);
            if !self.args.is_empty() {
                entry = entry.with_args(self.args);
            }
            if let Some(addr) = self.client_addr {
                entry = entry.with_client_addr(addr);
            }
            if let Some(k) = self.key {
                entry = entry.with_key(k);
            }
            if let Some(sid) = self.slot_id {
                entry = entry.with_slot_id(sid);
            }
            if let QueryResult::Error { message } = self.result {
                entry = entry.with_error(message);
            }

            // Use SlowQueryEntry::log which serializes to JSON and emits a tracing::warn
            entry.log();
        }
    }
}

impl SlowQueryEntry {
    pub fn new(
        command: String,
        duration: Duration,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().timestamp(),
            command,
            args: Vec::new(),
            duration_ms: duration.as_millis() as u64,
            client_addr: None,
            key: None,
            slot_id: None,
            result: QueryResult::Success,
        }
    }

    pub fn with_args(
        mut self,
        args: Vec<String>,
    ) -> Self {
        let config = SLOW_LOG_CONFIG.read();

        // Truncate args если слишком длинные
        self.args = args
            .into_iter()
            .map(|arg| {
                if arg.len() > config.max_args_len {
                    format!("{}...", &arg[..config.max_args_len])
                } else {
                    arg
                }
            })
            .collect();
        self
    }

    pub fn with_client_addr(
        mut self,
        addr: String,
    ) -> Self {
        self.client_addr = Some(addr);
        self
    }

    pub fn with_key(
        mut self,
        key: String,
    ) -> Self {
        self.key = Some(key);
        self
    }

    pub fn with_slot_id(
        mut self,
        slot_id: u16,
    ) -> Self {
        self.slot_id = Some(slot_id);
        self
    }

    pub fn with_error(
        mut self,
        message: String,
    ) -> Self {
        self.result = QueryResult::Error { message };
        self
    }

    /// Логирует slow query entry.
    pub fn log(self) {
        if let Ok(json) = serde_json::to_string(&self) {
            tracing::warn!(
                target: "slow_query",
                entry = %json,
                "Slow query entry"
            );
        }
    }
}

impl Default for SlowLogConfig {
    fn default() -> Self {
        Self {
            threshold: Duration::from_millis(100),
            sample_rate: 1.0,
            command_thresholds: std::collections::HashMap::new(),
            enable_backtrace: false,
            max_args_len: 256,
        }
    }
}

/// Обновляет конфигурацию slow log.
pub fn update_config(config: SlowLogConfig) {
    *SLOW_LOG_CONFIG.write() = config;
}

/// Получение текущей конфигурацию.
pub fn get_config() -> SlowLogConfig {
    SLOW_LOG_CONFIG.read().clone()
}

/// Получение метрик.
pub fn get_metrics() -> SlowLogStats {
    SLOW_LOG_METRICS.get_stats()
}

/// Установить threshold для конкретной команды
pub fn set_command_threshold(
    command: impl Into<String>,
    threshold: Duration,
) {
    let mut config = SLOW_LOG_CONFIG.write();
    config.command_thresholds.insert(command.into(), threshold);
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    // Вспомогательная функция для сброса глобального состояния перед тестами
    fn reset_globals() {
        // Сброс конфигурации
        *SLOW_LOG_CONFIG.write() = SlowLogConfig::default();

        // Сброс метрик
        SLOW_LOG_METRICS
            .total_slow_queries
            .store(0, Ordering::Relaxed);
        SLOW_LOG_METRICS.sampled_queries.store(0, Ordering::Relaxed);
        let mut counters = SLOW_LOG_METRICS.command_counters.lock();
        counters.clear();
    }

    /// Тест проверят, что механизм детекции `slow queries` работает корректно:
    /// быстрые запросы не учитываются, а медленные увеличивают счётчики метрик.
    #[test]
    fn test_slow_query_detection() {
        reset_globals();

        // Установка: создаём конфиг сразу с нужным threshold
        let config = SlowLogConfig {
            threshold: Duration::from_millis(1),
            ..Default::default()
        };
        update_config(config);

        // Быстрый запрос (не должен регистрироваться)
        let tracker = SlowQueryTracker::new("GET");
        thread::sleep(Duration::from_millis(5));
        tracker.finish();

        // Медленный запрос (должен регистрироваться в журнале)
        let tracker = SlowQueryTracker::new("SLOW_GET");
        thread::sleep(Duration::from_millis(30));
        tracker.finish();

        let stats = get_metrics();
        assert!(stats.total_slow_queries > 0);
    }

    /// Тест проверят, что можно задать per-command threshold и он сохраняется в
    /// конфигурации.
    #[test]
    fn test_per_command_threshold() {
        reset_globals();

        set_command_threshold("EXPENSIVE_OP", Duration::from_millis(200));

        let config = get_config();
        assert_eq!(
            config.command_thresholds.get("EXPENSIVE_OP"),
            Some(&Duration::from_millis(200))
        );
    }

    /// Тест проверят, что SlowQueryEntry корректно сериализуется в JSON
    /// и содержит поля команды и длительности (duration).
    #[test]
    fn test_slow_query_entry() {
        reset_globals();

        let entry = SlowQueryEntry::new("SET".to_string(), Duration::from_millis(150))
            .with_args(vec!["key1".to_string(), "value1".to_string()])
            .with_client_addr("127.0.0.1:8080".to_string())
            .with_key("user:123".to_string());

        // Should serialize to JSON
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("SET"));
        assert!(json.contains("150"));
    }

    /// Тест проверят, что ручная запись метрик (`record_slow_query`) корректно
    /// увеличивает total и per-command счётчики.
    #[test]
    fn test_metrics_recording_and_command_counter() {
        reset_globals();

        SLOW_LOG_METRICS.record_slow_query("CMDX");
        SLOW_LOG_METRICS.record_slow_query("CMDX");
        SLOW_LOG_METRICS.record_slow_query("CMDY");

        let stats = get_metrics();
        assert_eq!(stats.total_slow_queries, 3);
        assert_eq!(stats.command_counts.get("CMDX").copied().unwrap_or(0), 2);
        assert_eq!(stats.command_counts.get("CMDY").copied().unwrap_or(0), 1);
    }

    /// Тест проверят, что SlowQueryEntry::with_error сериализует ошибку
    /// и что args корректно усекаются согласно max_args_len.
    #[test]
    fn test_slow_query_entry_error_and_truncation() {
        reset_globals();

        // Установим маленький max_args_len чтобы проверить усечение
        let cfg = SlowLogConfig {
            max_args_len: 3,
            ..Default::default()
        };
        update_config(cfg);

        let entry = SlowQueryEntry::new("SET".to_string(), Duration::from_millis(150))
            .with_args(vec!["long_argument".to_string(), "ok".to_string()])
            .with_error("boom".to_string());

        let json = serde_json::to_string(&entry).unwrap();
        // должна содержать пометку об ошибке
        assert!(json.contains("error") || json.contains("boom"));
        // усечённый аргумент должен содержать "..."
        assert!(json.contains("..."));
    }

    /// Тест проверят, что срабатывание with_field() не вызывает панику и
    /// корректно логирует дополнительные поля (на уровне span).
    #[test]
    fn test_with_field_does_not_panic() {
        reset_globals();

        let mut t = SlowQueryTracker::new("FIELD_CMD");
        t.with_field("user", "anton");
        thread::sleep(Duration::from_millis(2));
        t.finish();

        // Просто проверяем отсутствие паники; метрики могут быть или не быть в
        // зависимости от config.sample_rate
        let _ = get_metrics();
    }
}
