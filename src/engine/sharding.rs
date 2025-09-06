use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock, RwLock,
    },
    time::{Duration, Instant},
};

use siphasher::sip::SipHasher;

static MONO_START: OnceLock<Instant> = OnceLock::new();

/// Конфигурация для sharded индекса.
#[derive(Debug, Clone)]
pub struct ShardingConfig {
    /// Кол-во шардов (по-умолчанию: num_cpus * 2)
    pub num_shards: usize,
    /// Включить метрики per-shard (может влиять на производительность)
    pub enable_metrics: bool,
    /// Threshold для slow operation logging (в микросекундах)
    pub slow_operation_threshold_us: u64,
}

/// Метрики для одного шарда.
#[derive(Debug, Default)]
pub struct ShardMetrics {
    /// Кол-во ключей в шарде
    pub key_count: AtomicU64,
    /// Общее кол-во write операций
    pub read_ops: AtomicU64,
    /// Общее кол-во write операций
    pub write_ops: AtomicU64,
    /// Суммарное время read lock contention (наносекунд)
    pub read_lock_wait_ns: AtomicU64,
    /// Суммарное время write lock contention (наносекунды)
    pub write_lock_wait_ns: AtomicU64,
    /// Количество slow operations
    pub slow_ops: AtomicU64,
    /// Последний timestamp обновления метрик
    pub last_updated: AtomicU64,
}

/// Imutable snapshot метрик шарда для экспорта.
#[derive(Debug, Clone)]
pub struct ShardMetricsSnapshot {
    pub key_count: u64,
    pub read_op: u64,
    pub write_op: u64,
    pub avg_read_lock_wait_us: f64,
    pub avg_write_lock_wait_us: f64,
    pub slow_ops: u64,
    pub last_updated: u64,
}

/// Один шард с данными и метриками.
pub struct Shard<V> {
    /// Данные шарда, защищённые RwLock для concurrent reads
    pub data: RwLock<HashMap<Vec<u8>, V>>,
    /// Метрики шарда (опциональные)
    pub metrics: Option<ShardMetrics>,
    /// ID шарда для логирования
    pub id: usize,
    /// threshold для slow operations (микросекунды)
    pub slow_op_threshold_us: u64,
}

/// Sharded индекс для распределения данных по шардам.
pub struct ShardedIndex<V> {
    shards: Vec<Shard<V>>,
    #[allow(dead_code)]
    config: ShardingConfig,
    /// Hasher для консистентного хеширования
    hasher_seed: (u64, u64),
}

/// Глобальная статистика по всем шардам.
#[derive(Debug, Clone)]
pub struct GlobalShardStats {
    pub total_shards: usize,
    pub total_keys: u64,
    pub total_read_ops: u64,
    pub total_write_ops: u64,
    pub total_slow_ops: u64,
    pub avg_keys_per_shard: f64,
    pub max_shard_keys: u64,
    pub min_shard_keys: u64,
}

impl ShardMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_read_op(
        &self,
        duration: Duration,
    ) {
        self.read_ops.fetch_add(1, Ordering::Relaxed);
        self.read_lock_wait_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        self.update_timestamp();
    }

    pub fn record_write_op(
        &self,
        duration: Duration,
    ) {
        self.write_ops.fetch_add(1, Ordering::Relaxed);
        self.write_lock_wait_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        self.update_timestamp();
    }

    pub fn record_slow_op(&self) {
        self.slow_ops.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_key_count(&self) {
        self.key_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_key_count(&self) {
        self.key_count.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn update_timestamp(&self) {
        // Инициализируем старт монопроцесcного времени один раз
        let start = MONO_START.get_or_init(Instant::now);
        // Получаем наносекунды как u128
        let nanos_u128 = Instant::now().duration_since(*start).as_nanos();
        // Безопасно приводим в u64 (ограничиваем у64::MAX при переполнении)
        let nanos = if nanos_u128 > u64::MAX as u128 {
            u64::MAX
        } else {
            nanos_u128 as u64
        };
        self.last_updated.store(nanos, Ordering::Relaxed);
    }

    /// Получить shapshot метрик для мониторинга.
    pub fn snapshot(&self) -> ShardMetricsSnapshot {
        let read_ops = self.read_ops.load(Ordering::Relaxed);
        let write_ops = self.write_ops.load(Ordering::Relaxed);
        let total_read_ns = self.read_lock_wait_ns.load(Ordering::Relaxed);
        let total_write_ns = self.write_lock_wait_ns.load(Ordering::Relaxed);

        ShardMetricsSnapshot {
            key_count: self.key_count.load(Ordering::Relaxed),
            read_op: read_ops,
            write_op: write_ops,
            avg_read_lock_wait_us: if read_ops > 0 {
                (total_read_ns as f64) / (read_ops as f64) / 1000.0
            } else {
                0.0
            },
            avg_write_lock_wait_us: if write_ops > 0 {
                (total_write_ns as f64) / (write_ops as f64) / 1000.0
            } else {
                0.0
            },
            slow_ops: self.slow_ops.load(Ordering::Relaxed),
            last_updated: self.last_updated.load(Ordering::Relaxed),
        }
    }
}

impl<V> Shard<V> {
    pub fn new(
        id: usize,
        enable_metrics: bool,
        slow_op_threshold_us: u64,
    ) -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            metrics: if enable_metrics {
                Some(ShardMetrics::new())
            } else {
                None
            },
            id,
            slow_op_threshold_us,
        }
    }

    /// Выполняет read операции с tracking метрик.
    pub fn read<F, R>(
        &self,
        f: F,
    ) -> R
    where
        F: FnOnce(&HashMap<Vec<u8>, V>) -> R,
    {
        let start = Instant::now();
        let guard = self.data.read().unwrap();
        let res = f(&*guard);
        drop(guard);

        if let Some(ref metrics) = self.metrics {
            metrics.record_read_op(start.elapsed());
        }
        res
    }

    /// Выполняет write операцию с tracking метрик.
    pub fn write<F, R>(
        &self,
        f: F,
    ) -> R
    where
        F: FnOnce(&mut HashMap<Vec<u8>, V>) -> R,
    {
        let start = Instant::now();
        let mut guard = self.data.write().unwrap();
        let res = f(&mut *guard);
        drop(guard);

        let elapsed = start.elapsed();
        if let Some(ref m) = self.metrics {
            m.record_write_op(elapsed);
            // Проверяем slow operation threshold
            if elapsed.as_micros() > self.slow_op_threshold_us as u128 {
                m.record_slow_op();
            }
        }
        res
    }
}

impl<V> ShardedIndex<V> {
    pub fn new(config: ShardingConfig) -> Self {
        let mut shards = Vec::with_capacity(config.num_shards);
        for i in 0..config.num_shards {
            shards.push(Shard::new(
                i,
                config.enable_metrics,
                config.slow_operation_threshold_us,
            ));
        }

        // Seed для SipHash — генерируем случайно (можно сделать детерминированным позже).
        let hasher_seed = (fastrand::u64(..), fastrand::u64(..));

        Self {
            shards,
            config,
            hasher_seed,
        }
    }

    /// Вычисляет номер шарда для ключа (консистентное хеширование)
    pub fn shard_for_key(
        &self,
        key: &[u8],
    ) -> usize {
        let mut hasher = SipHasher::new_with_keys(self.hasher_seed.0, self.hasher_seed.1);
        key.hash(&mut hasher);
        let hash = hasher.finish() as usize;
        let n = self.shards.len();
        if n.is_power_of_two() {
            hash & (n - 1)
        } else {
            hash % n
        }
    }

    /// Получает шард для кдюча.
    pub fn get_shard(
        &self,
        key: &[u8],
    ) -> &Shard<V> {
        let id = self.shard_for_key(key);
        &self.shards[id]
    }

    /// Возвращает все шарды (для операций типа mget).
    pub fn all_shards(&self) -> &[Shard<V>] {
        &self.shards
    }

    /// Группирует ключи по шардам.
    pub fn group_keys_by_shard<'a>(
        &self,
        keys: &'a [&'a [u8]],
    ) -> HashMap<usize, Vec<&'a [u8]>> {
        let mut groups: HashMap<usize, Vec<&'a [u8]>> = HashMap::new();
        for &k in keys {
            let sid = self.shard_for_key(k);
            groups.entry(sid).or_default().push(k);
        }
        groups
    }

    /// Вставка одного ключа. Возвращает Option<old_value>.
    /// Обновляет key_count метрику, если ключ новый.
    pub fn insert(
        &self,
        key: &[u8],
        value: V,
    ) -> Option<V> {
        let sid = self.shard_for_key(key);
        let shard = &self.shards[sid];
        let key_vec = key.to_vec();
        // возвращаем результат напрямую
        shard.write(|map| {
            let prev = map.insert(key_vec, value);
            if prev.is_none() {
                if let Some(m) = &shard.metrics {
                    m.increment_key_count();
                }
            }
            prev
        })
    }

    /// Удаление ключа, возвращает `true` если ключ существовал.
    pub fn remove(
        &self,
        key: &[u8],
    ) -> bool {
        let sid = self.shard_for_key(key);
        let shard = &self.shards[sid];
        // возвращаем результат напрямую
        shard.write(|map| {
            let existed = map.remove(key).is_some();
            if existed {
                if let Some(m) = &shard.metrics {
                    m.decrement_key_count();
                }
            }
            existed
        })
    }

    /// Получение значения по ключу. Требует V: Clone.
    pub fn get(
        &self,
        key: &[u8],
    ) -> Option<V>
    where
        V: Clone,
    {
        let sid = self.shard_for_key(key);
        let shard = &self.shards[sid];
        shard.read(|map| map.get(key).cloned())
    }

    /// Множественная вставка. Вход: Vec<(owned_key, value)>.
    /// Захватывает шарды в порядке их id (sorted), чтобы избежать дедлоков.
    pub fn mset(
        &self,
        entries: Vec<(Vec<u8>, V)>,
    ) {
        if entries.is_empty() {
            return;
        }
        // group per shard
        let mut groups: HashMap<usize, Vec<(Vec<u8>, V)>> = HashMap::new();
        for (k, v) in entries {
            let sid = self.shard_for_key(&k);
            groups.entry(sid).or_default().push((k, v));
        }
        // process shards in ascending order
        let mut shard_ids: Vec<usize> = groups.keys().cloned().collect();
        shard_ids.sort_unstable();
        for sid in shard_ids {
            let shard = &self.shards[sid];
            let list = groups.remove(&sid).unwrap_or_default();
            shard.write(|map| {
                for (k, v) in list {
                    let existed = map.insert(k, v);
                    if existed.is_none() {
                        if let Some(m) = &shard.metrics {
                            m.increment_key_count();
                        }
                    }
                }
            });
        }
    }

    /// Множественное чтение: возвращает Vec<Option<V>> в том же порядке, что и keys.
    pub fn mget(
        &self,
        keys: &[&[u8]],
    ) -> Vec<Option<V>>
    where
        V: Clone,
    {
        if keys.is_empty() {
            return vec![];
        }

        // 1) Группируем: shard_id -> Vec<(original_index, key_slice)>
        let mut groups: HashMap<usize, Vec<(usize, &[u8])>> = HashMap::new();
        for (i, &k) in keys.iter().enumerate() {
            let sid = self.shard_for_key(k);
            groups.entry(sid).or_default().push((i, k));
        }

        // 2) Результат — заранее зарезервированный вектор
        let mut results: Vec<Option<V>> = vec![None; keys.len()];

        // 3) Проходим по шардам в детерминированном порядке
        let mut shard_ids: Vec<usize> = groups.keys().cloned().collect();
        shard_ids.sort_unstable();

        for sid in shard_ids {
            let shard = &self.shards[sid];
            // берем список (index, key) для этого шарда
            let list = &groups[&sid];
            // читаем один раз и заполняем результаты по индексам
            shard.read(|map| {
                for &(idx, key) in list {
                    // HashMap<Vec<u8>, V> умеет `get(&[u8])` благодаря Borrow<[u8]>
                    results[idx] = map.get(key).cloned();
                }
            });
        }

        results
    }

    /// Собирает метрики со всех шардов.
    pub fn collect_metrics(&self) -> Vec<ShardMetricsSnapshot> {
        self.shards
            .iter()
            .filter_map(|shard| shard.metrics.as_ref().map(|m| m.snapshot()))
            .collect()
    }

    /// Возвращает общую статистику по всем шардам.
    pub fn global_stats(&self) -> GlobalShardStats {
        let snapshots = self.collect_metrics();

        let total_keys: u64 = snapshots.iter().map(|s| s.key_count).sum();
        let total_read_ops: u64 = snapshots.iter().map(|s| s.read_op).sum();
        let total_write_ops: u64 = snapshots.iter().map(|s| s.write_op).sum();
        let total_slow_ops: u64 = snapshots.iter().map(|s| s.slow_ops).sum();

        let max_shard_keys = snapshots.iter().map(|s| s.key_count).max().unwrap_or(0);
        let min_shard_keys = snapshots.iter().map(|s| s.key_count).min().unwrap_or(0);

        let avg = if !snapshots.is_empty() {
            total_keys as f64 / snapshots.len() as f64
        } else {
            0.0
        };

        GlobalShardStats {
            total_shards: self.shards.len(),
            total_keys,
            total_read_ops,
            total_write_ops,
            total_slow_ops,
            avg_keys_per_shard: avg,
            max_shard_keys,
            min_shard_keys,
        }
    }

    pub fn num_shards(&self) -> usize {
        self.shards.len()
    }
}

impl GlobalShardStats {
    /// Проверяет балансировку шардов (неравномерность распределения ключей).
    pub fn balance_ratio(&self) -> f64 {
        if self.min_shard_keys == 0 {
            return f64::INFINITY;
        }
        self.max_shard_keys as f64 / self.min_shard_keys as f64
    }

    /// Процент slow операций.
    pub fn slow_ops_percentage(&self) -> f64 {
        let total_ops = self.total_read_ops + self.total_write_ops;
        if total_ops > 0 {
            (self.total_slow_ops as f64 / total_ops as f64) * 100.0
        } else {
            0.0
        }
    }
}

impl Default for ShardingConfig {
    fn default() -> Self {
        Self {
            num_shards: num_cpus::get() * 2,
            enable_metrics: true,
            slow_operation_threshold_us: 1000, // 1мс
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет, что один и тот же ключ всегда попадает в один и тот же шард
    #[test]
    fn test_sharding_consistency() {
        let config = ShardingConfig {
            num_shards: 4,
            enable_metrics: false,
            slow_operation_threshold_us: 1000,
        };
        let index = ShardedIndex::<Vec<u8>>::new(config);

        let key = b"test_key";
        let shard1 = index.shard_for_key(key);
        let shard2 = index.shard_for_key(key);

        assert_eq!(shard1, shard2);
    }

    /// Тест проверяет равномерность распределения большого количества ключей по шардам
    #[test]
    fn test_key_distribution() {
        let config = ShardingConfig {
            num_shards: 4,
            enable_metrics: false,
            slow_operation_threshold_us: 1000,
        };
        let index = ShardedIndex::<Vec<u8>>::new(config);

        let mut shard_counts = vec![0; 4];

        for i in 0..1000 {
            let key = format!("key_{i}");
            let shard_id = index.shard_for_key(key.as_bytes());
            shard_counts[shard_id] += 1;
        }

        for count in shard_counts {
            assert!((200..300).contains(&count), "Uneven distribution: {count}");
        }
    }

    /// Тест проверяет корректность метрик при выполнении операций чтения и записи
    #[test]
    fn test_shard_metrics() {
        let config = ShardingConfig {
            num_shards: 2,
            enable_metrics: true,
            slow_operation_threshold_us: 1000,
        };
        let index = ShardedIndex::<Vec<u8>>::new(config);

        let shard = &index.shards[0];

        shard.read(|_data| {
            // имитация работы
            std::thread::sleep(Duration::from_micros(100));
        });

        shard.write(|_data| {
            // имитация работы
            std::thread::sleep(Duration::from_micros(100));
        });

        let metrics = shard.metrics.as_ref().unwrap().snapshot();

        assert_eq!(metrics.read_op, 1);
        assert_eq!(metrics.write_op, 1);
        assert!(metrics.avg_read_lock_wait_us > 0.0);
        assert!(metrics.avg_write_lock_wait_us > 0.0);
    }

    /// Тест проверяет корректность групповых операций mset и mget
    #[test]
    fn test_key_grouping() {
        let config = ShardingConfig {
            num_shards: 3,
            enable_metrics: false,
            slow_operation_threshold_us: 1000,
        };
        let index = ShardedIndex::<Vec<u8>>::new(config);

        // mset
        let entries = vec![
            (b"key1".to_vec(), b"v1".to_vec()),
            (b"key2".to_vec(), b"v2".to_vec()),
            (b"key3".to_vec(), b"v3".to_vec()),
        ];
        index.mset(entries);

        // mget
        let keys: Vec<&[u8]> = vec![b"key1", b"key2", b"key3"];
        let res = index.mget(&keys);
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].as_deref(), Some(b"v1".as_ref()));
        assert_eq!(res[1].as_deref(), Some(b"v2".as_ref()));
        assert_eq!(res[2].as_deref(), Some(b"v3".as_ref()));
    }

    /// Тест проверяет агрегацию глобальной статистики по всем шардам
    #[test]
    fn test_global_stats() {
        let config = ShardingConfig {
            num_shards: 2,
            enable_metrics: true,
            slow_operation_threshold_us: 1000,
        };
        let index = ShardedIndex::<Vec<u8>>::new(config);

        // Добавляем ключи в разные шарды через метрики
        index.shards[0]
            .metrics
            .as_ref()
            .unwrap()
            .increment_key_count();
        index.shards[0]
            .metrics
            .as_ref()
            .unwrap()
            .increment_key_count();
        index.shards[1]
            .metrics
            .as_ref()
            .unwrap()
            .increment_key_count();

        let stats = index.global_stats();
        assert_eq!(stats.total_shards, 2);
        assert_eq!(stats.total_keys, 3);
        assert_eq!(stats.avg_keys_per_shard, 1.5);
        assert_eq!(stats.max_shard_keys, 2);
        assert_eq!(stats.min_shard_keys, 1);
        assert_eq!(stats.balance_ratio(), 2.0);
    }

    /// Тест проверяет корректность удаления ключей и обработку повторного удаления
    #[test]
    fn test_remove_key() {
        let config = ShardingConfig::default();
        let index = ShardedIndex::<Vec<u8>>::new(config);

        // Вставляем ключ
        index.insert(b"foo", b"bar".to_vec());
        assert_eq!(index.get(b"foo"), Some(b"bar".to_vec()));

        // Удаляем ключ
        let removed = index.remove(b"foo");
        assert!(removed);

        // Ключа больше нет
        assert_eq!(index.get(b"foo"), None);

        // Повторное удаление -> false
        assert!(!index.remove(b"foo"));
    }

    /// Тест проверяет работу с пограничными случаями (пустой ключ, длинный ключ, пустые mset/mget)
    #[test]
    fn test_edge_cases() {
        let config = ShardingConfig::default();
        let index = ShardedIndex::<Vec<u8>>::new(config);

        // Пустой ключ
        index.insert(b"", b"empty".to_vec());
        assert_eq!(index.get(b""), Some(b"empty".to_vec()));

        // Очень длинный ключ (10 KB)
        let long_key = vec![b'x'; 10_000];
        index.insert(&long_key, b"big".to_vec());
        assert_eq!(index.get(&long_key), Some(b"big".to_vec()));

        // mset/mget с пустым списком
        index.mset(vec![]);
        let res = index.mget(&[]);
        assert_eq!(res.len(), 0);
    }

    /// Тест проверяет учёт "медленных" операций в метриках
    #[test]
    fn test_slow_ops_tracking() {
        let mut config = ShardingConfig {
            enable_metrics: true,
            slow_operation_threshold_us: 10, // 10 мкс
            ..Default::default()
        };
        config.enable_metrics = true;
        config.slow_operation_threshold_us = 10; // 10 мкс
        let index = ShardedIndex::<Vec<u8>>::new(config);
        let shard = &index.shards[0];

        // Выполним (медленную) запись
        shard.write(|_map| {
            std::thread::sleep(Duration::from_micros(50));
        });

        let metrics = shard.metrics.as_ref().unwrap().snapshot();
        assert_eq!(
            metrics.slow_ops, 1,
            "Должен зафиксироваться хотя бы один slow op"
        );
    }

    /// Тест проверяет корректность конкурентного доступа из нескольких потоков
    #[test]
    fn test_concurrent_access() {
        use std::{sync::Arc, thread};

        let config = ShardingConfig::default();
        let index = Arc::new(ShardedIndex::<u64>::new(config));

        let mut handles = vec![];

        // 4 потока, каждый пишет 1000 ключей
        for t in 0..4 {
            let idx = Arc::clone(&index);
            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    let key = format!("k{t}_{i}");
                    idx.insert(key.as_bytes(), i as u64);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Проверяем, что все ключи на месте
        for t in 0..4 {
            for i in 0..1000 {
                let key = format!("k{t}_{i}");
                let val = index.get(key.as_bytes());
                assert_eq!(val, Some(i as u64));
            }
        }

        let stats = index.global_stats();
        assert_eq!(stats.total_keys, 4000);
    }

    #[test]
    fn mget_handles_duplicates_and_order() {
        let cfg = ShardingConfig::default();
        let idx = ShardedIndex::<Vec<u8>>::new(cfg);
        idx.insert(b"dupe", b"x".to_vec());
        let keys: Vec<&[u8]> = vec![b"dupe", b"other", b"dupe"];
        let res = idx.mget(&keys);
        assert_eq!(res[0].as_deref(), Some(b"x".as_ref()));
        assert_eq!(res[2].as_deref(), Some(b"x".as_ref()));
    }

    #[test]
    fn last_updated_monotonic() {
        let cfg = ShardingConfig::default();
        let idx = ShardedIndex::<Vec<u8>>::new(cfg);
        let shard = &idx.shards[0];
        let start_ts = shard
            .metrics
            .as_ref()
            .unwrap()
            .last_updated
            .load(Ordering::Relaxed);
        shard.read(|_| {});
        let mid_ts = shard
            .metrics
            .as_ref()
            .unwrap()
            .last_updated
            .load(Ordering::Relaxed);
        assert!(mid_ts >= start_ts);
        std::thread::sleep(std::time::Duration::from_micros(10));
        shard.write(|_| {});
        let end_ts = shard
            .metrics
            .as_ref()
            .unwrap()
            .last_updated
            .load(Ordering::Relaxed);
        assert!(end_ts >= mid_ts);
    }

    #[test]
    fn metrics_match_map_len_after_concurrent_ops() {
        use std::{sync::Arc, thread};

        let cfg = ShardingConfig {
            num_shards: 4,
            enable_metrics: true,
            ..Default::default()
        };
        let idx = Arc::new(ShardedIndex::<u64>::new(cfg));

        let mut handles = vec![];
        for t in 0..8 {
            let idx = Arc::clone(&idx);
            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    let k = format!("k{t}_{i}");
                    idx.insert(k.as_bytes(), i as u64);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let mut real = 0u64;
        for shard in &idx.shards {
            let map_len = shard.data.read().unwrap().len() as u64;
            real += map_len;
        }
        let metrics_total: u64 = idx.collect_metrics().iter().map(|s| s.key_count).sum();
        assert_eq!(real, metrics_total);
    }
}
