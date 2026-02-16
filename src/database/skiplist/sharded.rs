use std::{
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::Instant,
};

use super::SkipList;

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
    wait_time_ns: AtomicUsize,
}

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
        let elapsed = start.elapsed().as_nanos() as usize;

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
        let elapsed = start.elapsed().as_nanos() as usize;

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
        let elapsed = start.elapsed().as_nanos() as usize;

        self.shard_metrics[shard_idx]
            .wait_time_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        self.shard_metrics[shard_idx]
            .removes
            .fetch_add(1, Ordering::Relaxed);

        let result = guard.remove(key);

        if result.is_some() {
            self.total_length.fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    pub fn len(&self) -> usize {
        self.total_length.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        let variance: f64 = distribution
            .iter()
            .map(|&count| {
                let diff = count as f64 - ideal_per_shard;
                diff * diff
            })
            .sum();

        let max_variance = ideal_per_shard * ideal_per_shard * self.num_shards as f64;

        if max_variance == 0.0 {
            1.0
        } else {
            1.0 - (variance / max_variance)
        }
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
}
