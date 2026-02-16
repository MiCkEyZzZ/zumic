use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, RwLock, RwLockReadGuard,
    },
    time::{Duration, Instant},
};

use super::{SkipList, ValidationError};

#[derive(Debug)]
pub struct ConcurrentSkipList<K, V> {
    inner: Arc<RwLock<SkipList<K, V>>>,
    cached_length: Arc<AtomicUsize>,
    metrics: Arc<ContentionMetrics>,
}

#[derive(Debug, Default)]
pub struct ContentionMetrics {
    pub read_locks: AtomicUsize,
    pub write_locks: AtomicUsize,
    pub lock_failures: AtomicUsize,
    pub total_wait_time_ns: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentionSnapshot {
    pub read_locks: usize,
    pub write_locks: usize,
    pub lock_failures: usize,
    pub total_wait_time_ns: u64,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<K, V> ConcurrentSkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SkipList::new())),
            cached_length: Arc::new(AtomicUsize::new(0)),
            metrics: Arc::new(ContentionMetrics::default()),
        }
    }

    pub fn insert(
        &self,
        key: K,
        value: V,
    ) {
        let start = Instant::now();
        let mut guard = self.inner.write().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_write(elapsed);

        let old_len = guard.len();
        guard.insert(key, value);
        let new_len = guard.len();

        // Обновляем cached length только если изменился
        if new_len != old_len {
            self.cached_length.store(new_len, Ordering::Relaxed);
        }
    }

    pub fn search(
        &self,
        key: &K,
    ) -> Option<V> {
        let start = Instant::now();
        let guard = self.inner.read().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_read(elapsed);

        guard.search(key).cloned()
    }

    pub fn remove(
        &self,
        key: &K,
    ) -> Option<V> {
        let start = Instant::now();
        let mut guard = self.inner.write().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_write(elapsed);

        let result = guard.remove(key);

        if result.is_some() {
            let new_len = guard.len();
            self.cached_length.store(new_len, Ordering::Relaxed);
        }

        result
    }

    pub fn len(&self) -> usize {
        self.cached_length.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(
        &self,
        key: &K,
    ) -> bool {
        self.search(key).is_some()
    }

    pub fn clear(&self) {
        let start = Instant::now();
        let mut guard = self.inner.write().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_write(elapsed);

        guard.clear();

        self.cached_length.store(0, Ordering::Relaxed);
    }

    pub fn first(&self) -> Option<(K, V)> {
        let start = Instant::now();
        let guard = self.inner.read().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_read(elapsed);

        guard.first().map(|(k, v)| (k.clone(), v.clone()))
    }

    pub fn last(&self) -> Option<(K, V)> {
        let start = Instant::now();
        let guard = self.inner.read().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_read(elapsed);

        guard.last().map(|(k, v)| (k.clone(), v.clone()))
    }

    pub fn validate_invariants(&self) -> Result<(), ValidationError> {
        let guard = self.inner.read().unwrap();
        guard.validate_invariants()
    }

    pub fn metrics(&self) -> ContentionSnapshot {
        ContentionSnapshot {
            read_locks: self.metrics.read_locks.load(Ordering::Relaxed),
            write_locks: self.metrics.write_locks.load(Ordering::Relaxed),
            lock_failures: self.metrics.lock_failures.load(Ordering::Relaxed),
            total_wait_time_ns: self.metrics.total_wait_time_ns.load(Ordering::Relaxed),
        }
    }

    pub fn try_search(
        &self,
        key: &K,
        timeout: Duration,
    ) -> Option<V> {
        let start = Instant::now();

        // Простая реализация timeout через polling
        // В позже можно использовать parking_lot::RwLock с try_read_for()
        loop {
            if let Ok(guard) = self.inner.try_read() {
                let elapsed = start.elapsed().as_nanos() as u64;

                self.metrics.inc_read(elapsed);

                return guard.search(key).cloned();
            }

            if start.elapsed() > timeout {
                self.metrics.inc_failure();
                return None;
            }

            std::thread::yield_now();
        }
    }

    pub fn try_insert(
        &self,
        key: K,
        value: V,
        timeout: Duration,
    ) -> bool {
        let start = std::time::Instant::now();

        loop {
            if let Ok(mut guard) = self.inner.try_write() {
                let elapsed = start.elapsed().as_nanos() as u64;

                self.metrics.inc_write(elapsed);

                let old_len = guard.len();
                guard.insert(key, value);
                let new_len = guard.len();

                if new_len != old_len {
                    self.cached_length.store(new_len, Ordering::Relaxed);
                }

                return true;
            }

            if start.elapsed() > timeout {
                self.metrics.inc_failure();
                return false;
            }

            std::thread::yield_now();
        }
    }

    pub fn with_read<F, R>(
        &self,
        f: F,
    ) -> R
    where
        F: FnOnce(RwLockReadGuard<SkipList<K, V>>) -> R,
    {
        let start = Instant::now();
        let guard = self.inner.read().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_read(elapsed);

        f(guard)
    }

    pub fn with_write<F, R>(
        &self,
        f: F,
    ) -> R
    where
        F: FnOnce(&mut SkipList<K, V>) -> R,
    {
        let start = Instant::now();
        let mut guard = self.inner.write().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics.inc_write(elapsed);

        let result = f(&mut guard);

        self.cached_length.store(guard.len(), Ordering::Relaxed);

        result
    }

    pub fn reset_metrics(&self) {
        self.metrics.reset();
    }

    pub fn snapshot_and_reset(&self) -> ContentionSnapshot {
        // Снимаем текущие значения
        ContentionSnapshot {
            read_locks: self.metrics.read_locks.swap(0, Ordering::Relaxed),
            write_locks: self.metrics.write_locks.swap(0, Ordering::Relaxed),
            lock_failures: self.metrics.lock_failures.swap(0, Ordering::Relaxed),
            total_wait_time_ns: self.metrics.total_wait_time_ns.swap(0, Ordering::Relaxed),
        }
    }
}

impl ContentionSnapshot {
    pub fn average_wait_time_ns(&self) -> f64 {
        let total_locks = self.read_locks + self.write_locks;

        if total_locks == 0 {
            0.0
        } else {
            self.total_wait_time_ns as f64 / total_locks as f64
        }
    }

    pub fn read_write_ratio(&self) -> f64 {
        if self.write_locks == 0 {
            f64::INFINITY
        } else {
            self.read_locks as f64 / self.write_locks as f64
        }
    }

    pub fn failure_rate(&self) -> f64 {
        let total_attempts = self.read_locks + self.write_locks + self.lock_failures;

        if total_attempts == 0 {
            0.0
        } else {
            self.lock_failures as f64 / total_attempts as f64
        }
    }

    pub fn total_locks(&self) -> usize {
        self.read_locks + self.write_locks
    }

    pub fn contention_rate(&self) -> f64 {
        let locks = self.total_locks();

        if locks == 0 {
            0.0
        } else {
            self.lock_failures as f64 / locks as f64
        }
    }

    pub fn avg_wait_time_us(&self) -> f64 {
        self.average_wait_time_ns() / 1000.0
    }

    pub fn avg_wait_time_ms(&self) -> f64 {
        self.average_wait_time_ns() / 1_000_000.0
    }

    pub fn total_attempts(&self) -> usize {
        self.read_locks + self.write_locks + self.lock_failures
    }

    pub fn success_rate(&self) -> f64 {
        let attempts = self.total_attempts();

        if attempts == 0 {
            1.0
        } else {
            self.total_locks() as f64 / attempts as f64
        }
    }

    pub fn is_contended(&self) -> bool {
        self.lock_failures > 0
    }

    pub fn average_wait_duration(&self) -> Duration {
        Duration::from_nanos(self.average_wait_time_ns() as u64)
    }

    pub fn format_report(&self) -> String {
        format!(
            "Contention Metrics:\n\
                 Read locks: {}\n\
                 Write locks: {}\n\
                 Lock failures: {}\n\
                 Total attempts: {}\n\
                 Success rate: {:.2}%\n\
                 Failure rate: {:.2}%\n\
                 R/W ratio: {:.2}\n\
                 Avg wait time: {:.2} µs\n",
            self.read_locks,
            self.write_locks,
            self.lock_failures,
            self.total_attempts(),
            self.success_rate() * 100.0,
            self.failure_rate() * 100.0,
            self.read_write_ratio(),
            self.avg_wait_time_us(),
        )
    }
}

impl ContentionMetrics {
    pub fn reset(&self) {
        self.read_locks.store(0, Ordering::Relaxed);
        self.write_locks.store(0, Ordering::Relaxed);
        self.lock_failures.store(0, Ordering::Relaxed);
        self.total_wait_time_ns.store(0, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn inc_read(
        &self,
        duration_ns: u64,
    ) {
        self.read_locks.fetch_add(1, Ordering::Relaxed);
        self.total_wait_time_ns
            .fetch_add(duration_ns, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn inc_write(
        &self,
        duration_ns: u64,
    ) {
        self.write_locks.fetch_add(1, Ordering::Relaxed);
        self.total_wait_time_ns
            .fetch_add(duration_ns, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn inc_failure(&self) {
        self.lock_failures.fetch_add(1, Ordering::Relaxed);
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для ConcurrentSkipList
////////////////////////////////////////////////////////////////////////////////

impl<K, V> Clone for ConcurrentSkipList<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            cached_length: Arc::clone(&self.cached_length),
            metrics: Arc::clone(&self.metrics),
        }
    }
}

impl<K, V> Default for ConcurrentSkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::{sync::Barrier, thread, time::Duration};

    use super::*;

    #[test]
    fn test_basic_concurrent_operations() {
        let list = ConcurrentSkipList::new();

        list.insert(1, "one");
        list.insert(2, "two");

        assert_eq!(list.search(&1), Some("one"));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_concurrent_insert_search() {
        let list = ConcurrentSkipList::new();

        let writer_list = list.clone();
        let reader_list = list.clone();

        let writer = thread::spawn(move || {
            for i in 0..1000 {
                writer_list.insert(i, i * 2);
            }
        });

        let reader = thread::spawn(move || {
            for i in 0..1000 {
                let _ = reader_list.search(&i);
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();

        assert_eq!(list.len(), 1000);
    }

    #[test]
    fn test_multiple_readers() {
        let list = ConcurrentSkipList::new();
        let mut handles = vec![];

        // Заполняем данными
        for i in 0..100 {
            list.insert(i, i);
        }

        // 10 concurrent readers
        for _ in 0..10 {
            let list_c = list.clone();

            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    assert_eq!(list_c.search(&i), Some(i));
                }
            }))
        }

        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_metrics() {
        let list = ConcurrentSkipList::new();

        list.insert(1, "one");
        list.search(&1);
        list.remove(&1);

        let metrics = list.metrics();

        assert!(metrics.read_locks > 0);
        assert!(metrics.write_locks > 0);
    }

    #[test]
    fn test_try_operations_timeout() {
        let list = ConcurrentSkipList::new();

        // try_insert должен успеть
        assert!(list.try_insert(1, "one", Duration::from_secs(1)));

        // try_search должен успеть
        assert_eq!(list.try_search(&1, Duration::from_secs(1)), Some("one"));
    }

    #[test]
    fn test_concurrent_remove_clear() {
        let list = ConcurrentSkipList::new();

        for i in 0..100 {
            list.insert(i, i);
        }

        let list1 = list.clone();
        let list2 = list.clone();

        let remover = thread::spawn(move || {
            for i in 0..50 {
                list1.remove(&i);
            }
        });

        let cleaner = thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            list2.clear();
        });

        remover.join().unwrap();
        cleaner.join().unwrap();

        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn test_cached_length_consistency() {
        let list = ConcurrentSkipList::new();

        for i in 0..1000 {
            list.insert(i, i);
        }

        assert_eq!(list.len(), 1000);

        for i in 0..500 {
            list.remove(&i);
        }

        assert_eq!(list.len(), 500);
    }

    #[test]
    fn test_first_last_concurrent() {
        let list = ConcurrentSkipList::new();

        for i in 1..=100 {
            list.insert(i, i);
        }

        let list_r = list.clone();
        let reader = thread::spawn(move || {
            for _ in 0..50 {
                let f = list_r.first().unwrap();
                let l = list_r.last().unwrap();
                assert!(f.0 <= l.0);
            }
        });

        reader.join().unwrap();
    }

    #[test]
    fn test_try_operations_contention() {
        let list = ConcurrentSkipList::new();

        let mut handles = vec![];

        for _ in 0..5 {
            let list_c = list.clone();
            handles.push(thread::spawn(move || {
                for j in 0..100 {
                    let _ = list_c.try_insert(j, j, Duration::from_millis(1));
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Проверяем, что cached_length <= 100
        assert!(list.len() <= 100);
    }

    #[test]
    fn test_metrics_under_load() {
        let list = ConcurrentSkipList::new();

        let mut handles = vec![];

        for _ in 0..10 {
            let list_c = list.clone();
            handles.push(thread::spawn(move || {
                for j in 0..100 {
                    list_c.insert(j, j);
                    let _ = list_c.search(&j);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let metrics = list.metrics();
        assert!(metrics.read_locks > 0);
        assert!(metrics.write_locks > 0);
        assert!(metrics.total_wait_time_ns > 0);
    }

    #[test]
    fn test_validate_invariants_after_operations() {
        let list = ConcurrentSkipList::new();

        for i in 0..1000 {
            list.insert(i, i);
        }

        for i in 0..500 {
            list.remove(&i);
        }

        assert!(list.validate_invariants().is_ok());
    }

    #[test]
    fn test_validate_invariants_concurrent() {
        let list = ConcurrentSkipList::new();
        let mut handles = vec![];

        for t in 0..4 {
            let list_c = list.clone();
            handles.push(thread::spawn(move || {
                for i in 0..500 {
                    list_c.insert(i + t * 1000, i);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert!(list.validate_invariants().is_ok());
    }

    #[test]
    fn test_with_read_access() {
        let list = ConcurrentSkipList::new();

        list.insert(10, 20);

        let result = list.with_read(|guard| guard.search(&10).cloned());

        assert_eq!(result, Some(20));
    }

    #[test]
    fn test_with_read_concurrent() {
        let list = ConcurrentSkipList::new();
        let mut handles = vec![];

        for i in 0..100 {
            list.insert(i, i);
        }

        for _ in 0..8 {
            let list_c = list.clone();
            handles.push(thread::spawn(move || {
                list_c.with_read(|guard| {
                    for i in 0..100 {
                        assert_eq!(guard.search(&i).cloned(), Some(i));
                    }
                })
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_with_write_insert() {
        let list = ConcurrentSkipList::new();

        list.with_write(|guard| {
            guard.insert(1, 100);
        });

        assert_eq!(list.search(&1), Some(100));
    }

    #[test]
    fn test_with_write_concurrent() {
        let list = ConcurrentSkipList::new();
        let mut handles = vec![];

        for t in 0..4 {
            let list_c = list.clone();

            handles.push(thread::spawn(move || {
                list_c.with_write(|guard| {
                    for i in 0..100 {
                        guard.insert(i + t * 1000, i);
                    }
                });
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(list.len(), 400);
    }

    #[test]
    fn test_with_write_cached_length_consistency_after_mutations() {
        let list = ConcurrentSkipList::new();

        list.with_write(|guard| {
            for i in 0..100 {
                guard.insert(i, i);
            }
        });

        assert_eq!(list.len(), 100);

        list.with_write(|guard| {
            for i in 0..50 {
                guard.remove(&i);
            }
        });

        assert_eq!(list.len(), 50);

        list.with_write(|guard| {
            guard.clear();
        });

        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_invariants_under_heavy_concurrency() {
        let list = ConcurrentSkipList::new();
        let mut handles = vec![];

        for t in 0..8 {
            let list_c = list.clone();

            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    list_c.insert(i + t * 10000, i);

                    if i % 2 == 0 {
                        list_c.remove(&(i + t * 10000));
                    }
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert!(list.validate_invariants().is_ok());
    }

    #[test]
    fn test_with_write_cached_length_consistency() {
        let list = ConcurrentSkipList::new();

        list.with_write(|guard| {
            for i in 0..100 {
                guard.insert(i, i);
            }
        });

        assert_eq!(list.len(), 100);
    }

    #[test]
    fn test_snapshot_total_attempts() {
        let snapshot = ContentionSnapshot {
            read_locks: 10,
            write_locks: 5,
            lock_failures: 2,
            total_wait_time_ns: 1000,
        };

        assert_eq!(snapshot.total_attempts(), 17);
    }

    #[test]
    fn test_snapshot_success_rate() {
        let snapshot = ContentionSnapshot {
            read_locks: 8,
            write_locks: 2,
            lock_failures: 0,
            total_wait_time_ns: 1000,
        };

        assert_eq!(snapshot.success_rate(), 1.0);
    }

    #[test]
    fn test_snapshot_failure_rate() {
        let snapshot = ContentionSnapshot {
            read_locks: 8,
            write_locks: 2,
            lock_failures: 10,
            total_wait_time_ns: 1000,
        };

        assert!((snapshot.failure_rate() - 0.5).abs() < 0.0001);
    }

    #[test]
    fn test_snapshot_avg_wait() {
        let snapshot = ContentionSnapshot {
            read_locks: 5,
            write_locks: 5,
            lock_failures: 0,
            total_wait_time_ns: 1000,
        };

        assert_eq!(snapshot.average_wait_time_ns(), 100.0);
    }

    #[test]
    fn test_snapshot_is_contended() {
        let snapshot = ContentionSnapshot {
            read_locks: 5,
            write_locks: 5,
            lock_failures: 1,
            total_wait_time_ns: 1000,
        };

        assert!(snapshot.is_contended());
    }

    #[test]
    fn test_try_insert_timeout_records_failure() {
        let list = ConcurrentSkipList::new();
        let barrier = Arc::new(Barrier::new(2));
        let b = barrier.clone();
        let list_holder = list.clone();

        let holder = thread::spawn(move || {
            // внутри with_write мы держим write-lock до выхода из замыкания
            list_holder.with_write(|_guard| {
                // сигнализируем, что держим lock
                b.wait();
                // держим lock дольше, чем таймаут теста
                std::thread::sleep(Duration::from_millis(200));
            });
        });

        // ждём, пока holder реально захватит lock
        barrier.wait();

        // короткий таймаут - должен вернуть false и увеличить счётчик неудач
        let ok = list.try_insert(42, "v", Duration::from_millis(50));
        assert!(
            !ok,
            "try_insert must time out and return false under contention"
        );

        holder.join().unwrap();

        let snap = list.metrics();
        assert!(
            snap.lock_failures > 0,
            "lock_failures must be > 0 after try_insert timeout"
        );
    }

    #[test]
    fn test_try_search_timeout_records_failure() {
        let list = ConcurrentSkipList::new();
        // наполним список, чтобы search не возвращал None по другой причине
        list.insert(1, 1);

        let barrier = Arc::new(Barrier::new(2));
        let b = barrier.clone();
        let list_holder = list.clone();

        let holder = thread::spawn(move || {
            list_holder.with_write(|_g| {
                b.wait();
                std::thread::sleep(Duration::from_millis(200));
            });
        });

        barrier.wait();

        let res = list.try_search(&1, Duration::from_millis(50));
        assert_eq!(res, None);
        holder.join().unwrap();

        let snap = list.metrics();
        assert!(snap.lock_failures > 0);
    }

    #[test]
    fn test_reset_metrics_works() {
        let list = ConcurrentSkipList::new();
        list.insert(1, "one");
        list.search(&1);
        list.remove(&1);

        let before = list.metrics();
        assert!(before.read_locks > 0 || before.write_locks > 0 || before.total_wait_time_ns > 0);

        list.reset_metrics();
        let after = list.metrics();
        assert_eq!(after.read_locks, 0);
        assert_eq!(after.write_locks, 0);
        assert_eq!(after.lock_failures, 0);
        assert_eq!(after.total_wait_time_ns, 0);
    }

    #[test]
    fn test_snapshot_format_report_contains_fields() {
        let snap = ContentionSnapshot {
            read_locks: 3,
            write_locks: 1,
            lock_failures: 1,
            total_wait_time_ns: 10_000,
        };
        let s = snap.format_report();
        assert!(s.contains("Read locks"));
        assert!(s.contains("Write locks"));
        assert!(s.contains("Avg wait"));
    }

    #[test]
    fn smoke_stress_no_panic() {
        use std::sync::Arc;
        let list = Arc::new(ConcurrentSkipList::new());
        let mut handles = vec![];
        for t in 0..8 {
            let l = Arc::clone(&list);
            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    l.insert(i + t * 10000, i);
                    if i % 3 == 0 {
                        let _ = l.search(&(i + t * 10000));
                    }
                    if i % 5 == 0 {
                        l.remove(&(i + t * 10000));
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // неassert на конкретное число — главное, чтобы не упало
    }
}
