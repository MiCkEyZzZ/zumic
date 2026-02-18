use std::{
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::Instant,
};

use super::SkipList;
use crate::ValidationError;

/// Кол-во shards по умолчанию.
const DEFAULT_SHARDS: usize = 16;

/// Максимальное количество шардов для очень больших систем.
const MAX_SHARDS: usize = 256;

#[derive(Debug)]
pub struct ShardedSkipList<K, V> {
    shards: Vec<Arc<RwLock<SkipList<K, V>>>>,
    num_shards: usize,
    total_length: Arc<AtomicUsize>,
    shard_metrics: Vec<Arc<ShardMetrics>>,
}

#[derive(Debug, Default)]
struct ShardMetrics {
    inserts: AtomicUsize,
    searches: AtomicUsize,
    removes: AtomicUsize,
    wait_time_ns: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
pub struct ShardMetricsSnapshot {
    pub shard_index: usize,
    pub inserts: usize,
    pub searches: usize,
    pub removes: usize,
    pub wait_time_ns: u64,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<K, V> ShardedSkipList<K, V>
where
    K: Ord + Clone + Default + Debug + Hash,
    V: Clone + Debug + Default,
{
    pub fn new() -> Self {
        Self::with_shards(DEFAULT_SHARDS)
    }

    pub fn with_shards(num_shards: usize) -> Self {
        assert!(
            num_shards > 0 && num_shards <= MAX_SHARDS,
            "num_shards must be in range (0, {MAX_SHARDS}]"
        );

        let shards: Vec<_> = (0..num_shards)
            .map(|_| Arc::new(RwLock::new(SkipList::new())))
            .collect();

        let shard_metrics: Vec<_> = (0..num_shards)
            .map(|_| Arc::new(ShardMetrics::default()))
            .collect();

        ShardedSkipList {
            shards,
            num_shards,
            total_length: Arc::new(AtomicUsize::new(0)),
            shard_metrics,
        }
    }

    pub fn insert(
        &self,
        key: K,
        value: V,
    ) {
        let shard_idx = self.shard_index(&key);
        let start = std::time::Instant::now();
        let mut guard = self.shards[shard_idx].write().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.shard_metrics[shard_idx]
            .wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.shard_metrics[shard_idx]
            .inserts
            .fetch_add(1, Ordering::Relaxed);

        let old_len = guard.len();

        guard.insert(key, value);

        let new_len = guard.len();

        if new_len > old_len {
            self.total_length.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn search(
        &self,
        key: &K,
    ) -> Option<V> {
        let shard_idx = self.shard_index(key);
        let start = Instant::now();
        let guard = self.shards[shard_idx].read().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.shard_metrics[shard_idx]
            .wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.shard_metrics[shard_idx]
            .searches
            .fetch_add(1, Ordering::Relaxed);

        guard.search(key).cloned()
    }

    pub fn remove(
        &self,
        key: &K,
    ) -> Option<V> {
        let shard_idx = self.shard_index(key);
        let start = Instant::now();
        let mut guard = self.shards[shard_idx].write().unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;

        self.shard_metrics[shard_idx]
            .wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.shard_metrics[shard_idx]
            .removes
            .fetch_add(1, Ordering::Relaxed);

        let result = guard.remove(key);

        if result.is_some() {
            self.total_length.fetch_sub(1, Ordering::Relaxed);
        }

        result
    }

    pub fn len(&self) -> usize {
        self.total_length.load(Ordering::Relaxed)
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
        for shard in &self.shards {
            let mut guard = shard.write().unwrap();
            guard.clear();
        }

        self.total_length.store(0, Ordering::Relaxed);
    }

    pub fn first(&self) -> Option<(K, V)> {
        let mut min: Option<(K, V)> = None;

        for shard in &self.shards {
            let guard = shard.read().unwrap();

            if let Some((k, v)) = guard.first() {
                let candidate = (k.clone(), v.clone());

                match &min {
                    Some((min_k, _)) if k < min_k => min = Some(candidate),
                    None => min = Some(candidate),
                    _ => {}
                }
            }
        }

        min
    }

    pub fn last(&self) -> Option<(K, V)> {
        let mut max: Option<(K, V)> = None;

        for shard in &self.shards {
            let guard = shard.read().unwrap();

            if let Some((k, v)) = guard.last() {
                let candidate = (k.clone(), v.clone());

                match &max {
                    Some((max_k, _)) if k > max_k => max = Some(candidate),
                    None => max = Some(candidate),
                    _ => {}
                }
            }
        }

        max
    }

    pub fn validate_invariants(&self) -> Result<(), ValidationError> {
        for (idx, shard) in self.shards.iter().enumerate() {
            let guard = shard.read().unwrap();

            guard
                .validate_invariants()
                .map_err(|e| ValidationError::SortOrderViolation {
                    message: format!("Shard {idx}: {e:?}"),
                })?;
        }

        Ok(())
    }

    pub fn shard_metrics(&self) -> Vec<ShardMetricsSnapshot> {
        self.shard_metrics
            .iter()
            .enumerate()
            .map(|(idx, metrics)| ShardMetricsSnapshot {
                shard_index: idx,
                inserts: metrics.inserts.load(Ordering::Relaxed),
                searches: metrics.searches.load(Ordering::Relaxed),
                removes: metrics.removes.load(Ordering::Relaxed),
                wait_time_ns: metrics.wait_time_ns.load(Ordering::Relaxed),
            })
            .collect()
    }

    pub fn shard_distribution(&self) -> Vec<usize> {
        self.shards
            .iter()
            .map(|shard| {
                let guard = shard.read().unwrap();
                guard.len()
            })
            .collect()
    }

    pub fn load_balance_score(&self) -> f64 {
        let distribution = self.shard_distribution();
        let total: usize = distribution.iter().sum();

        if total == 0 {
            return 1.0;
        }

        let ideal_per_shard = total as f64 / self.num_shards as f64;

        if ideal_per_shard == 0.0 {
            return 1.0;
        }

        let variance: f64 = distribution
            .iter()
            .map(|&count| {
                let diff = count as f64 - ideal_per_shard;
                diff * diff
            })
            .sum();

        let stddev = (variance / self.num_shards as f64).sqrt();
        let score = 1.0 - (stddev / ideal_per_shard);

        score.clamp(0.0, 1.0)
    }

    fn shard_index(
        &self,
        key: &K,
    ) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        (hash as usize) % self.num_shards
    }
}

impl ShardMetricsSnapshot {
    pub fn total_operations(&self) -> usize {
        self.inserts + self.searches + self.removes
    }

    pub fn average_wait_time_ns(&self) -> f64 {
        let total_ops = self.total_operations();

        if total_ops == 0 {
            0.0
        } else {
            self.wait_time_ns as f64 / total_ops as f64
        }
    }

    pub fn format_report(&self) -> String {
        format!(
            "Shard {}:\n\
                Inserts: {}\n\
                Searches: {}\n\
                Removes: {}\n\
                Total ops: {}\n\
                Avg wait: {:.2} ns\n",
            self.shard_index,
            self.inserts,
            self.searches,
            self.removes,
            self.total_operations(),
            self.average_wait_time_ns()
        )
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для ShardedSkipList
////////////////////////////////////////////////////////////////////////////////

impl<K, V> Clone for ShardedSkipList<K, V> {
    fn clone(&self) -> Self {
        ShardedSkipList {
            shards: self.shards.clone(),
            num_shards: self.num_shards,
            total_length: Arc::clone(&self.total_length),
            shard_metrics: self.shard_metrics.clone(),
        }
    }
}

impl<K, V> Default for ShardedSkipList<K, V>
where
    K: Ord + Clone + Default + Debug + Hash,
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
    use std::thread;

    use super::*;

    #[test]
    fn test_sharded_basic() {
        let list = ShardedSkipList::new();

        list.insert(1, "one");
        list.insert(2, "two");

        assert_eq!(list.search(&1), Some("one"));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_concurrent_sharded_inserts() {
        let list = ShardedSkipList::with_shards(8);
        let mut handles = vec![];

        // 8 потоков, каждый вставляет в разные shards
        for i in 0..8 {
            let list = list.clone();
            handles.push(thread::spawn(move || {
                for j in 0..1000 {
                    list.insert(i * 1000 + j, j);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(list.len(), 8000);
    }

    #[test]
    fn test_load_balance() {
        let list = ShardedSkipList::with_shards(4);

        // Равномерно распределяем ключи
        for i in 0..1000 {
            list.insert(i, i);
        }

        let score = list.load_balance_score();

        // Должно быть достаточно балансированным с хорошим hash
        assert!(score > 0.8, "Load balance score: {score}");
    }

    #[test]
    fn test_shard_distribution() {
        let list = ShardedSkipList::with_shards(4);

        for i in 0..100 {
            list.insert(i, i);
        }

        let dist = list.shard_distribution();

        assert_eq!(dist.len(), 4);

        let total: usize = dist.iter().sum();

        assert_eq!(total, 100);
    }

    #[test]
    fn test_first_last_across_shards() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(10, "ten");
        list.insert(1, "one");
        list.insert(100, "hundred");

        assert_eq!(list.first(), Some((1, "one")));
        assert_eq!(list.last(), Some((100, "hundred")))
    }

    #[test]
    fn test_concurrent_mixed_operations() {
        let list = ShardedSkipList::with_shards(16);
        let mut handles = vec![];

        // Пишем
        for i in 0..4 {
            let list_c = list.clone();
            handles.push(thread::spawn(move || {
                for j in 0..1000 {
                    list_c.insert(i * 1000 + j, j);
                }
            }));
        }

        // Читаем
        for _ in 0..4 {
            let list_c = list.clone();
            handles.push(thread::spawn(move || {
                for i in 0..4000 {
                    let _ = list_c.search(&i);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert!(!list.is_empty());
    }

    #[test]
    fn test_contains_and_clear() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(10, "ten");
        list.insert(20, "twenty");

        assert!(list.contains(&10));
        assert!(list.contains(&20));
        assert!(!list.contains(&30));

        list.clear();

        assert_eq!(list.len(), 0);
        assert!(!list.contains(&10));
        assert!(list.is_empty());
    }

    #[test]
    fn test_validate_invariants_on_empty_and_populated() {
        let list = ShardedSkipList::with_shards(8);

        // пустая - должна пройти
        assert!(list.validate_invariants().is_ok());

        for i in 0..500 {
            list.insert(i, i);
        }

        assert!(list.validate_invariants().is_ok());
    }

    #[test]
    fn test_shard_metrics_update() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(1, "one");
        list.insert(2, "two");
        let _ = list.search(&1);
        let _ = list.remove(&2);

        let metrics: Vec<_> = list
            .shard_metrics
            .iter()
            .map(|m| {
                (
                    m.inserts.load(Ordering::Relaxed),
                    m.searches.load(Ordering::Relaxed),
                    m.removes.load(Ordering::Relaxed),
                    m.wait_time_ns.load(Ordering::Relaxed),
                )
            })
            .collect();

        let total_inserts: usize = metrics.iter().map(|(i, ..)| *i).sum();
        let total_searches: usize = metrics.iter().map(|(_, s, ..)| *s).sum();
        let total_removes: usize = metrics.iter().map(|(_, _, r, _)| *r).sum();

        assert_eq!(total_inserts, 2);
        assert_eq!(total_searches, 1);
        assert_eq!(total_removes, 1);

        // wait_time_ns должен быть >0 для всех шардов, где были операции
        assert!(metrics.iter().any(|(_, _, _, t)| *t > 0));
    }

    #[test]
    fn test_first_last_empty_shards() {
        let list = ShardedSkipList::<i32, i32>::with_shards(4);
        assert_eq!(list.first(), None);
        assert_eq!(list.last(), None);

        list.insert(42, 100);
        assert_eq!(list.first(), Some((42, 100)));
        assert_eq!(list.last(), Some((42, 100)));
    }

    #[test]
    fn test_high_concurrency() {
        use std::sync::Arc;
        let list = Arc::new(ShardedSkipList::with_shards(16));
        let mut handles = vec![];

        // 16 потоков пишут
        for i in 0..16 {
            let list = Arc::clone(&list);
            handles.push(thread::spawn(move || {
                for j in 0..1000 {
                    list.insert(i * 1000 + j, j);
                }
            }));
        }

        // 16 потоков читают
        for _ in 0..16 {
            let list = Arc::clone(&list);
            handles.push(thread::spawn(move || {
                for j in 0..16000 {
                    let _ = list.search(&j);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Проверяем, что длина правильная
        assert_eq!(list.len(), 16 * 1000);
    }

    #[test]
    fn test_clear_full_load() {
        let list = ShardedSkipList::with_shards(4);
        for i in 0..1000 {
            list.insert(i, i);
        }

        assert_eq!(list.len(), 1000);
        list.clear();
        assert_eq!(list.len(), 0);
        for shard_len in list.shard_distribution() {
            assert_eq!(shard_len, 0);
        }
    }

    #[test]
    fn test_insert_overwrites_value() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(1, "one");
        assert_eq!(list.len(), 1);
        assert_eq!(list.search(&1), Some("one"));

        list.insert(1, "uno");
        assert_eq!(list.len(), 1); // len не увеличился
        assert_eq!(list.search(&1), Some("uno")); // значение обновилось
    }

    #[test]
    fn test_contains_after_remove() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(10, "ten");
        assert!(list.contains(&10));

        let removed = list.remove(&10);
        assert_eq!(removed, Some("ten"));
        assert!(!list.contains(&10));
    }

    #[test]
    fn test_first_last_after_removals() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(1, "one");
        list.insert(2, "two");
        list.insert(3, "three");

        assert_eq!(list.first(), Some((1, "one")));
        assert_eq!(list.last(), Some((3, "three")));

        list.remove(&1);
        assert_eq!(list.first(), Some((2, "two")));

        list.remove(&3);
        assert_eq!(list.last(), Some((2, "two")));
    }

    #[test]
    fn test_clear_after_partial_removal() {
        let list = ShardedSkipList::with_shards(4);

        for i in 0..10 {
            list.insert(i, i);
        }

        list.remove(&0);
        list.remove(&9);

        assert_eq!(list.len(), 8);
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn test_shard_metrics_consistency() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(1, "a");
        list.insert(2, "b");
        let _ = list.search(&1);
        let _ = list.search(&3); // несуществующий
        let _ = list.remove(&2);
        let _ = list.remove(&4); // несуществующий

        let total_wait: u64 = list
            .shard_metrics
            .iter()
            .map(|m| m.wait_time_ns.load(Ordering::Relaxed))
            .sum();

        assert!(total_wait > 0);
    }

    #[test]
    fn test_shard_metrics_snapshot_consistency() {
        let list = Arc::new(ShardedSkipList::with_shards(8));

        let mut handles = vec![];

        for i in 0..8 {
            let list = Arc::clone(&list);
            handles.push(thread::spawn(move || {
                for j in 0..1000 {
                    list.insert(i * 1000 + j, j);
                    let _ = list.search(&(i * 1000 + j));
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let snapshot = list.shard_metrics();

        let total_inserts: usize = snapshot.iter().map(|s| s.inserts).sum();
        let total_searches: usize = snapshot.iter().map(|s| s.searches).sum();

        assert_eq!(total_inserts, 8000);
        assert_eq!(total_searches, 8000);
    }

    #[test]
    fn test_shard_metrics_snapshot_is_immutable() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(1, 1);

        let snapshot1 = list.shard_metrics();

        list.insert(2, 2);

        let snapshot2 = list.shard_metrics();

        let total1: usize = snapshot1.iter().map(|s| s.inserts).sum();
        let total2: usize = snapshot2.iter().map(|s| s.inserts).sum();

        assert_eq!(total1, 1);
        assert_eq!(total2, 2);
    }

    #[test]
    fn test_clone_shares_state() {
        let list = ShardedSkipList::with_shards(4);

        list.insert(1, "one");

        let cloned = list.clone();

        cloned.insert(2, "two");

        assert_eq!(list.len(), 2);
        assert_eq!(cloned.len(), 2);

        assert_eq!(list.search(&2), Some("two"));
    }

    #[test]
    fn test_load_balance_score_edge_cases() {
        let list = ShardedSkipList::<i32, i32>::with_shards(4);

        // empty case
        assert_eq!(list.load_balance_score(), 1.0);

        // single element
        list.insert(1, 1);

        let score = list.load_balance_score();

        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn test_shard_metrics_format_report() {
        let list = ShardedSkipList::with_shards(2);

        list.insert(1, 1);

        let snapshot = list.shard_metrics();
        let reposrt = snapshot[0].format_report();

        assert!(reposrt.contains("Shard"));
        assert!(reposrt.contains("Inserts"));
    }
}
