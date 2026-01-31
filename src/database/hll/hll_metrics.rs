use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct HllMetrics {
    inner: Arc<HllMetricsInner>,
}

#[derive(Debug)]
pub struct HllMetricsInner {
    total_created: AtomicUsize,
    sparse_count: AtomicUsize,
    dense_count: AtomicUsize,
    sparse_to_dense_conversions: AtomicU64,
    total_adds: AtomicU64,
    total_merges: AtomicU64,
    total_estimations: AtomicU64,
    total_memory_bytes: AtomicUsize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HllMetricsSnapshot {
    pub total_created: usize,
    pub sparse_count: usize,
    pub dense_count: usize,
    pub sparse_to_dense_conversions: u64,
    pub total_adds: u64,
    pub total_merges: u64,
    pub total_estimations: u64,
    pub total_memory_bytes: usize,
}

impl HllMetrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(HllMetricsInner {
                total_created: AtomicUsize::new(0),
                sparse_count: AtomicUsize::new(0),
                dense_count: AtomicUsize::new(0),
                sparse_to_dense_conversions: AtomicU64::new(0),
                total_adds: AtomicU64::new(0),
                total_merges: AtomicU64::new(0),
                total_estimations: AtomicU64::new(0),
                total_memory_bytes: AtomicUsize::new(0),
            }),
        }
    }

    /// Регистрирует создание нового HLL (sparse).
    pub fn on_create_sparse(&self) {
        self.inner.total_created.fetch_add(1, Ordering::Relaxed);
        self.inner.sparse_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Регистрирует конверсию sparse->dense.
    // ВАЖНО:
    // sparse_count уменьшается безопасно, без риска underflow.
    // Не используем `fetch_sub(1)`, потому что при конкурентных вызовах
    // счётчик мог бы уйти из 0 в `usize::MAX`.
    // Вместо этого применяем `fetch_update` с `saturating_sub(1)`,
    // что гарантирует:
    // - счётчик никогда не станет отрицательным
    // - корректное поведение даже при гонках потоков
    // - отсутствие недетерминированного состояния.
    pub fn on_sparse_to_dense_conversion(
        &self,
        memory_increase: usize,
    ) {
        // Атомарно уменьшаем количество sparse HLL,
        // не допуская underflow при значении 0.
        self.inner
            .sparse_count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            })
            .ok();

        // Увеличиваем количество dense HLL
        self.inner.dense_count.fetch_add(1, Ordering::Relaxed);
        // Регистрируем сам факт конверсии
        self.inner
            .sparse_to_dense_conversions
            .fetch_add(1, Ordering::Relaxed);
        // Учитываем рост потребляемой памяти
        self.inner
            .total_memory_bytes
            .fetch_add(memory_increase, Ordering::Relaxed);
    }

    /// Регестрирует операции `add`
    pub fn on_add(&self) {
        self.inner.total_adds.fetch_add(1, Ordering::Relaxed);
    }

    /// Регестрирует операцию `merge`.
    pub fn on_merge(&self) {
        self.inner.total_merges.fetch_add(1, Ordering::Relaxed);
    }

    /// Регестрирует оценку кардинальности.
    pub fn on_estimation(&self) {
        self.inner.total_estimations.fetch_add(1, Ordering::Relaxed);
    }

    /// Обновляет суммарное потребление памяти.
    // ВАЖНО: используется `fetch_update` с saturating-арифметикой, чтобы обеспечить
    // детерминированное и безопасное поведенеи при конкурентных обновлениях.
    // Причины:
    // - `total_memory_bytes` может как увеличиваться, так и уменьшаться
    // - прямое использование `fetch_add` / `fetch_sub` небезопасно при уменьшении
    //   возможно underflow (переход из 0 в `usize::MAX`)
    // - `fetch_update` реализует атомарный load + CAS-цикл, гарантируя корректное
    //   обновление даже при гонках потоков.
    // Поведение:
    // - при `delta > 0` память увеличивается;
    // - при `delta < 0` память уменьшается, но не ниже 0;
    // - счётчик никогда не переполняется и не уходит в некорректное состояние.
    // Ordering::Relaxed используется осознанно:
    // это метрика, а не инвариант логики,
    // строгая синхронизация между потоками не требуется.
    pub fn update_memory(
        &self,
        delta: isize,
    ) {
        self.inner
            .total_memory_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                if delta >= 0 {
                    Some(v.saturating_add(delta as usize))
                } else {
                    Some(v.saturating_sub((-delta) as usize))
                }
            })
            .ok();
    }

    pub fn snapshot(&self) -> HllMetricsSnapshot {
        HllMetricsSnapshot {
            total_created: self.inner.total_created.load(Ordering::Relaxed),
            sparse_count: self.inner.sparse_count.load(Ordering::Relaxed),
            dense_count: self.inner.dense_count.load(Ordering::Relaxed),
            sparse_to_dense_conversions: self
                .inner
                .sparse_to_dense_conversions
                .load(Ordering::Relaxed),
            total_adds: self.inner.total_adds.load(Ordering::Relaxed),
            total_merges: self.inner.total_merges.load(Ordering::Relaxed),
            total_estimations: self.inner.total_estimations.load(Ordering::Relaxed),
            total_memory_bytes: self.inner.total_memory_bytes.load(Ordering::Relaxed),
        }
    }

    /// Сбрасывает все метрики в начальное состояние.
    pub fn reset(&self) {
        self.inner.total_created.store(0, Ordering::Relaxed);
        self.inner.sparse_count.store(0, Ordering::Relaxed);
        self.inner.dense_count.store(0, Ordering::Relaxed);
        self.inner
            .sparse_to_dense_conversions
            .store(0, Ordering::Relaxed);
        self.inner.total_adds.store(0, Ordering::Relaxed);
        self.inner.total_merges.store(0, Ordering::Relaxed);
        self.inner.total_estimations.store(0, Ordering::Relaxed);
        self.inner.total_memory_bytes.store(0, Ordering::Relaxed);
    }
}

impl Default for HllMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = HllMetrics::new();
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.total_created, 0);
        assert_eq!(snapshot.sparse_count, 0);
        assert_eq!(snapshot.dense_count, 0);
    }

    #[test]
    fn test_on_create_sparse() {
        let metrics = HllMetrics::new();

        metrics.on_create_sparse();
        metrics.on_create_sparse();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_created, 2);
        assert_eq!(snapshot.sparse_count, 2);
        assert_eq!(snapshot.dense_count, 0);
    }

    #[test]
    fn test_on_sparse_to_dense_conversion() {
        let metrics = HllMetrics::new();

        metrics.on_create_sparse();
        metrics.on_sparse_to_dense_conversion(12288);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.sparse_count, 0);
        assert_eq!(snapshot.dense_count, 1);
        assert_eq!(snapshot.sparse_to_dense_conversions, 1);
        assert_eq!(snapshot.total_memory_bytes, 12288);
    }

    #[test]
    fn test_operations_tracking() {
        let metrics = HllMetrics::new();

        metrics.on_add();
        metrics.on_add();
        metrics.on_merge();
        metrics.on_estimation();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_adds, 2);
        assert_eq!(snapshot.total_merges, 1);
        assert_eq!(snapshot.total_estimations, 1);
    }

    #[test]
    fn test_memory_update() {
        let metrics = HllMetrics::new();

        metrics.update_memory(1000);
        metrics.update_memory(500);
        metrics.update_memory(-200);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_memory_bytes, 1300);
    }

    #[test]
    fn test_reset() {
        let metrics = HllMetrics::new();

        metrics.on_create_sparse();
        metrics.on_add();
        metrics.on_merge();

        let snapshot_before = metrics.snapshot();
        assert!(snapshot_before.total_created > 0);

        metrics.reset();

        let snapshot_after = metrics.snapshot();
        assert_eq!(snapshot_after.total_created, 0);
        assert_eq!(snapshot_after.total_adds, 0);
        assert_eq!(snapshot_after.total_merges, 0);
    }
}
