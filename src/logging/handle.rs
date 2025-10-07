use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use tracing_appender::non_blocking::WorkerGuard;

/// Метрики для LoggingHandle.
#[derive(Debug, Default)]
pub struct LoggingMetrics {
    /// Кол-во droped messages (приблизительно)
    pub dropped_messages: AtomicU64,
    /// Флаг активного shutdown
    pub shutdown_in_progress: AtomicBool,
    /// Кол-во flush операций
    pub flush_count: AtomicU64,
}

/// Handle для управления lifecycle логирования.
pub struct LoggingHandle {
    /// File guard (обязательный, если file logging включён)
    _file_guard: Option<WorkerGuard>,
    /// Network guard (опциональный, для будущего)
    _network_guard: Option<WorkerGuard>,
    /// Метрики логирования
    pub metrics: Arc<LoggingMetrics>,
    /// Timeout для flush при shutdown (по умолчанию 5 секунд)
    flush_timeout: Duration,
}

/// Статистика логирования.
#[derive(Debug, Clone, Copy)]
pub struct LoggingStats {
    pub dropped_messages: u64,
    pub flush_count: u64,
    pub shutdown_in_progress: bool,
}

impl LoggingMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_dropped(
        &self,
        count: u64,
    ) {
        self.dropped_messages.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_flush(&self) {
        self.flush_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_dropped_messages(&self) -> u64 {
        self.dropped_messages.load(Ordering::Relaxed)
    }

    pub fn get_flush_count(&self) -> u64 {
        self.flush_count.load(Ordering::Relaxed)
    }

    pub fn is_shutdown_in_progress(&self) -> bool {
        self.shutdown_in_progress.load(Ordering::Relaxed)
    }

    fn start_shutdown(&self) {
        self.shutdown_in_progress.store(true, Ordering::Release);
    }
}

impl LoggingHandle {
    /// Создаёт новый LoggingHandle.
    pub fn new(
        file_guard: Option<WorkerGuard>,
        network_guard: Option<WorkerGuard>,
    ) -> Self {
        Self {
            _file_guard: file_guard,
            _network_guard: network_guard,
            metrics: Arc::new(LoggingMetrics::new()),
            flush_timeout: Duration::from_secs(5),
        }
    }

    /// Устанавливает custom flush timeout.
    pub fn with_flush_timeout(
        mut self,
        timeout: Duration,
    ) -> Self {
        self.flush_timeout = timeout;
        self
    }

    /// Принудительный flush всех буферов (неблокирующий).
    pub fn flush(&self) {
        // WorkerGuard автоматически делает flush при drop,
        // но мы можем записать это в метрики
        self.metrics.record_flush();

        // TODO: В будущем можно добавить explicit flush API
        // если tracing-appender будет поддерживать
        tracing::debug!(
            flush_count = self.metrics.get_flush_count(),
            "Logging flush requested"
        );
    }

    /// Graceful shutdown с таймаутом.
    pub fn shutdown(mut self) {
        self.metrics.start_shutdown();

        let dropped = self.metrics.get_dropped_messages();
        let flushes = self.metrics.get_flush_count();

        tracing::info!(
            dropped_messages = dropped,
            total_flushes = flushes,
            timeout_secs = self.flush_timeout.as_secs(),
            "Initiating logging shutdown"
        );

        let start = std::time::Instant::now();

        drop(self._file_guard.take());
        drop(self._network_guard.take());

        let elapsed = start.elapsed();

        if elapsed > self.flush_timeout {
            eprintln!(
                "WARNING: Logging shutdown took {}ms (timeout: {}ms)",
                elapsed.as_millis(),
                self.flush_timeout.as_millis()
            );
        } else {
            tracing::info!(
                shutdown_duration_ms = elapsed.as_millis(),
                "Logging shutdown completed"
            );
        }
    }

    /// Shutdown с явным таймаутом (для async контекстов)
    pub async fn shutdown_async(
        mut self,
        timeout: Duration,
    ) {
        self.metrics.start_shutdown();

        let dropped = self.metrics.get_dropped_messages();

        tracing::info!(
            dropped_messages = dropped,
            timeout_ms = timeout.as_millis(),
            "Async logging shutdown initiated"
        );

        // Берем Option::take() для guard-ов, чтобы безопасно вызвать drop
        let file_guard = self._file_guard.take();
        let network_guard = self._network_guard.take();

        // Выполняем shutdown в блокирующем потоке с таймаутом
        match tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || {
                drop(file_guard);
                drop(network_guard);
            }),
        )
        .await
        {
            Ok(Ok(())) => {
                tracing::info!("Async logging shutdown completed successfully");
            }
            Ok(Err(e)) => {
                eprintln!("Logging shutdown task panicked: {}", e);
            }
            Err(_) => {
                eprintln!(
                    "WARNING: Logging shutdown exceeded timeout of {}ms",
                    timeout.as_millis()
                );
            }
        }
    }

    /// Получить текущие метрики
    pub fn get_metrics(&self) -> LoggingStats {
        LoggingStats {
            dropped_messages: self.metrics.get_dropped_messages(),
            flush_count: self.metrics.get_flush_count(),
            shutdown_in_progress: self.metrics.is_shutdown_in_progress(),
        }
    }

    /// Проверить, идёт ли shutdown
    pub fn is_shutdown_in_progress(&self) -> bool {
        self.metrics.is_shutdown_in_progress()
    }
}

impl Drop for LoggingHandle {
    fn drop(&mut self) {
        if !self.metrics.is_shutdown_in_progress() {
            // Если shutdown не был вызван явно, логируем warning
            eprintln!(
                "WARNING: LoggingHandle dropped without explicit shutdown(). \
                 Some logs may be lost. Call .shutdown() for graceful cleanup."
            )
        }
    }
}

// SAFETY: LoggingHandle is Send + Sync because WorkerGuard is Send + Sync
unsafe impl Send for LoggingHandle {}
unsafe impl Sync for LoggingHandle {}
