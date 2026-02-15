use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::{Duration, Instant},
};

use super::SkipList;

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

        self.metrics
            .total_wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.metrics.write_locks.fetch_add(1, Ordering::Relaxed);

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

        self.metrics
            .total_wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.metrics.read_locks.fetch_add(1, Ordering::Relaxed);

        guard.search(key).cloned()
    }

    pub fn remove(
        &self,
        key: &K,
    ) -> Option<V> {
        let start = Instant::now();
        let mut guard = self.inner.write().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.metrics
            .total_wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.metrics.write_locks.fetch_add(1, Ordering::Relaxed);

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

        self.metrics
            .total_wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.metrics.write_locks.fetch_add(1, Ordering::Relaxed);

        guard.clear();

        self.cached_length.store(0, Ordering::Relaxed);
    }

    pub fn first(&self) -> Option<(K, V)> {
        let guard = self.inner.read().unwrap();
        guard.first().map(|(k, v)| (k.clone(), v.clone()))
    }

    pub fn last(&self) -> Option<(K, V)> {
        let guard = self.inner.read().unwrap();
        guard.last().map(|(k, v)| (k.clone(), v.clone()))
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
                self.metrics
                    .total_wait_time_ns
                    .fetch_add(elapsed, Ordering::Relaxed);
                self.metrics.read_locks.fetch_add(1, Ordering::Relaxed);

                return guard.search(key).cloned();
            }

            if start.elapsed() > timeout {
                self.metrics.lock_failures.fetch_add(1, Ordering::Relaxed);
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
                self.metrics
                    .total_wait_time_ns
                    .fetch_add(elapsed, Ordering::Relaxed);
                self.metrics.write_locks.fetch_add(1, Ordering::Relaxed);

                let old_len = guard.len();
                guard.insert(key, value);
                let new_len = guard.len();

                if new_len != old_len {
                    self.cached_length.store(new_len, Ordering::Relaxed);
                }

                return true;
            }

            if start.elapsed() > timeout {
                self.metrics.lock_failures.fetch_add(1, Ordering::Relaxed);
                return false;
            }

            std::thread::yield_now();
        }
    }
}

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

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

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
}
