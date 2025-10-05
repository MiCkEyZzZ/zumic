use std::{
    collections::{hash_map::DefaultHasher, HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use crate::{Result as SmResult, SlotManagerError};

/// Идентификатор шарда (node) в кластере.
pub type ShardId = usize;
/// Идентификатор слота (0..16383).
pub type SlotId = u16;

/// Количество слотов (фиксировано, совместимо с Redis).
const TOTAL_SLOTS: u16 = 16384;
/// Коэффициент для принятия решения о ребалансинге:
/// если max_load / min_load > REBALANCE_THRESHOLD → рассматриваем ребаланс.
const REBALANCE_THRESHOLD: f64 = 1.5;
/// Порог доступа к слоту, выше которого слот считается "hot".
const HOT_KEY_THRESHOLD: u64 = 100;
/// Максимальное количество слотов, планируемых к миграции за одну операцию.
const MIGRATION_BATCH_SIZE: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub enum SlotState {
    Stable(ShardId),
    Migrating {
        from: ShardId,
        to: ShardId,
        progress: f64, // от 0.0 до 1.0
    },
    Importing {
        from: ShardId,
        to: ShardId,
        progress: f64,
    },
}

/// Агрегированные метрики нагрузки, которые можно заполнять
/// периодическим агрегатором (background worker).
///
/// Поля:
/// - `operations_per_second` — приблизительные операции/секунду по каждому
///   шарду;
/// - `hot_keys` — map "ключ -> счётчик" (на текущий момент простая реализация);
/// - `slot_access_count` — агрегированное число обращений по слоту;
/// - `last_updated` — момент последнего обновления агрегата.
#[derive(Debug, Clone)]
pub struct LoadMetrics {
    pub operations_per_second: HashMap<ShardId, u64>,
    pub hot_keys: HashMap<String, u64>,
    pub slot_access_count: HashMap<SlotId, u64>,
    pub last_updated: Instant,
}

/// Задача миграции для одного слота.
/// Хранит прогресс миграции и список ключей, если он заполняется во время
/// сканирования шарда.
#[derive(Debug)]
pub struct MigrationTask {
    pub slot: SlotId,
    pub from_shard: ShardId,
    pub to_shard: ShardId,
    pub keys_to_migrate: Vec<String>,
    pub migrated_keys: usize,
    pub total_keys: usize,
    pub started_at: Instant,
}

/// Управляет присвоениями слотов, очередью миграций и метриками.
///
/// Поля (основные):
/// - `slot_assignments` — вектор длинной TOTAL_SLOTS, состояние каждого слота.
/// - `slot_map_version` — атомарная версия map'ы (инкрементируется при
///   изменениях).
/// - `shard_ops` — вектор атомиков по шардам (hot-path счётчики операций).
/// - `slot_access` — вектор атомиков по слотам (hot-path счётчики доступа).
/// - `load_metrics` — агрегированные метрики (для UI, operator API и тестов).
/// - `active_migrations` / `migration_queue` — управление миграциями.
#[derive(Debug)]
pub struct SlotManager {
    slot_assignments: Arc<RwLock<Vec<SlotState>>>,
    slot_map_version: Arc<AtomicU64>,
    shard_ops: Arc<Vec<AtomicU64>>,
    slot_access: Arc<Vec<AtomicU64>>,
    load_metrics: Arc<RwLock<LoadMetrics>>,
    #[allow(dead_code)]
    metrics_history: Arc<Mutex<VecDeque<LoadMetrics>>>,
    active_migrations: Arc<Mutex<HashMap<SlotId, MigrationTask>>>,
    migration_queue: Arc<Mutex<VecDeque<(SlotId, ShardId, ShardId)>>>,
    #[allow(dead_code)]
    shard_count: usize,
    rebalance_interval: Duration,
    last_rebalance: Arc<Mutex<Instant>>,
    #[allow(dead_code)]
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
}

/// Небольшая реализация consistent-hash кольца для альтернативной
/// маршрутизации. Хранит map hash -> shard и отсортированный массив хешей для
/// быстрых обходов.
#[derive(Debug)]
pub struct ConsistentHashRing {
    nodes: HashMap<u64, ShardId>,
    sorted_hashes: Vec<u64>,
    virtual_nodes_per_shard: usize,
}

impl ConsistentHashRing {
    /// Создание кольца с `shard_count` узлами и `virtual_nodes_per_shard`
    /// виртуальными нодами. Использует `DefaultHasher` (SipHash или
    /// реализация std).
    pub fn new(
        shard_count: usize,
        virtual_nodes_per_shard: usize,
    ) -> Self {
        let mut nodes = HashMap::new();

        for shard_id in 0..shard_count {
            for virtual_node in 0..virtual_nodes_per_shard {
                let mut hasher = DefaultHasher::new();
                format!("shard-{}-vnode-{}", shard_id, virtual_node).hash(&mut hasher);
                let hash = hasher.finish();
                nodes.insert(hash, shard_id);
            }
        }

        let mut sorted_hashes: Vec<u64> = nodes.keys().cloned().collect();
        sorted_hashes.sort_unstable();

        Self {
            nodes,
            sorted_hashes,
            virtual_nodes_per_shard,
        }
    }

    /// Поиск шарда по строковому ключу.
    ///
    /// Примечание: это упрощённая реализация; для больших колец имеет смысл
    /// избегать выделения/сортировки при каждом изменении, а также
    /// использовать более быстрый hasher.
    pub fn get_shard(
        &self,
        key: &str,
    ) -> ShardId {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let key_hash = hasher.finish();

        match self.sorted_hashes.binary_search(&key_hash) {
            Ok(idx) => self.nodes[&self.sorted_hashes[idx]],
            Err(idx) => {
                let pos = if idx == self.sorted_hashes.len() {
                    0
                } else {
                    idx
                };
                self.nodes[&self.sorted_hashes[pos]]
            }
        }
    }

    /// Добавление шарда в кольцо (пересобирает `sorted_hashes`).
    pub fn add_shard(
        &mut self,
        shard_id: ShardId,
    ) {
        for virtual_node in 0..self.virtual_nodes_per_shard {
            let mut hasher = DefaultHasher::new();
            format!("shard-{}-vnode-{}", shard_id, virtual_node).hash(&mut hasher);
            let hash = hasher.finish();
            self.nodes.insert(hash, shard_id);
        }
        let mut v: Vec<u64> = self.nodes.keys().cloned().collect();
        v.sort_unstable();
        self.sorted_hashes = v;
    }

    /// Удаление шарда из кольца (пересобирает `sorted_hashes`).
    pub fn remove_shard(
        &mut self,
        shard_id: ShardId,
    ) {
        self.nodes.retain(|_, &mut id| id != shard_id);
        let mut v: Vec<u64> = self.nodes.keys().cloned().collect();
        v.sort_unstable();
        self.sorted_hashes = v;
    }
}

impl SlotManager {
    /// Создание SlotManager с `shard_count` шардов и начальным round-robin
    /// распределением 16384 слотов между ними.
    ///
    /// Производительность:
    /// - `record_operation` — lock-free (атомики), подходит для высокочастотных
    ///   путей.
    /// - Операции управления slot_map используют `RwLock` (корректно для редких
    ///   изменений).
    pub fn new(shard_count: usize) -> Self {
        let mut vec_assignments = vec![SlotState::Stable(0); TOTAL_SLOTS as usize];
        for slot in 0..TOTAL_SLOTS {
            let shard_id = (slot as usize) % shard_count;
            vec_assignments[slot as usize] = SlotState::Stable(shard_id);
        }

        let shard_ops = Arc::new(
            (0..shard_count)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>(),
        );
        let slot_access = Arc::new(
            (0..TOTAL_SLOTS as usize)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>(),
        );

        Self {
            slot_assignments: Arc::new(RwLock::new(vec_assignments)),
            slot_map_version: Arc::new(AtomicU64::new(1)),
            shard_ops,
            slot_access,
            load_metrics: Arc::new(RwLock::new(LoadMetrics {
                operations_per_second: HashMap::new(),
                hot_keys: HashMap::new(),
                slot_access_count: HashMap::new(),
                last_updated: Instant::now(),
            })),
            metrics_history: Arc::new(Mutex::new(VecDeque::with_capacity(100))),
            active_migrations: Arc::new(Mutex::new(HashMap::new())),
            migration_queue: Arc::new(Mutex::new(VecDeque::new())),
            shard_count,
            rebalance_interval: Duration::from_secs(30),
            last_rebalance: Arc::new(Mutex::new(Instant::now())),
            hash_ring: Arc::new(RwLock::new(ConsistentHashRing::new(shard_count, 160))),
        }
    }

    /// Получение `ShardId`, обслуживающий конкретный слот.
    ///
    /// Возвращает `None`, если `slot` выходит за пределы TOTAL_SLOTS.
    pub fn get_slot_shard(
        &self,
        slot: SlotId,
    ) -> Option<ShardId> {
        let assignments = self.slot_assignments.read().unwrap();
        let idx = slot as usize;
        if idx >= assignments.len() {
            return None;
        }
        match &assignments[idx] {
            SlotState::Stable(s) => Some(*s),
            SlotState::Migrating { from, .. } => Some(*from),
            SlotState::Importing { to, .. } => Some(*to),
        }
    }

    /// Для ключа (строка в протоколе Redis) вычисляет шард.
    /// Берёт слот через `calculate_slot` и затем `get_slot_shard`.
    pub fn get_key_shard(
        &self,
        key: &str,
    ) -> ShardId {
        let slot = self.calculate_slot(key);
        self.get_slot_shard(slot).unwrap_or(0)
    }

    /// Вычисляет слот по ключу с поддержкой hash-tag синтаксиса `{tag}`:
    /// если ключ содержит `{...}`, то хешируется только содержимое внутри
    /// фигурных скобок.
    pub fn calculate_slot(
        &self,
        key: &str,
    ) -> SlotId {
        let hash_key = if let Some(start) = key.find('{') {
            if let Some(end) = key[start + 1..].find('}') {
                let tag = &key[start + 1..start + 1 + end];
                if !tag.is_empty() {
                    tag
                } else {
                    key
                }
            } else {
                key
            }
        } else {
            key
        };

        crc16(hash_key.as_bytes()) % TOTAL_SLOTS
    }

    /// Записывает факт операции по ключу — hot-path: только атомики.
    ///
    /// Важно: это не обновляет агрегированный `load_metrics` напрямую — он
    /// должен быть заполнен периодическим агрегатором (background worker),
    /// чтобы не тратить CPU в hot-path.
    pub fn record_operation(
        &self,
        key: &str,
    ) {
        let slot = self.calculate_slot(key) as usize;
        let shard_id = self.get_key_shard(key);

        if slot < self.slot_access.len() {
            self.slot_access[slot].fetch_add(1, Ordering::Relaxed);
        }
        if shard_id < self.shard_ops.len() {
            self.shard_ops[shard_id].fetch_add(1, Ordering::Relaxed);
        }

        // ПРИМЕЧАНИЕ: сознательно не обновляем HashMap в критическом/горячем
        // пути. Сбор и агрегация данных в load_metrics выполняются
        // периодически фоновым заданием.
    }

    /// Возвращает `true`, если по текущим метрикам стоит инициировать ребаланс.
    ///
    /// Правила:
    /// - Если присутствуют агрегированные метрики
    ///   (`load_metrics.operations_per_second`), они имеют приоритет.
    /// - Иначе используется быстрый fallback на атомики `shard_ops`.
    /// - Также учитывается интервал `rebalance_interval` — если недавно делался
    ///   ребаланс, пропускаем (кроме случая, когда у нас есть явные
    ///   агрегированные метрики).
    pub fn should_rebalance(&self) -> bool {
        // Если недавно уже делали ребаланс и нет агрегированных метрик - отбой.
        let last_rebalance = *self.last_rebalance.lock().unwrap();
        {
            let metrics = self.load_metrics.read().unwrap();
            if last_rebalance.elapsed() < self.rebalance_interval
                && metrics.operations_per_second.is_empty()
            {
                return false;
            }
        }

        // Если есть агрегированные операции в load_metrics, используем их (тесты и
        // background-агрегатор).
        {
            let metrics = self.load_metrics.read().unwrap();
            if !metrics.operations_per_second.is_empty() {
                let loads: Vec<u64> = metrics.operations_per_second.values().cloned().collect();
                if loads.len() < 2 {
                    return false;
                }
                let max_load = *loads.iter().max().unwrap() as f64;
                let min_load = *loads.iter().min().unwrap() as f64;
                if min_load == 0.0 {
                    return max_load > 0.0;
                }
                return (max_load / min_load) > REBALANCE_THRESHOLD;
            }
        }

        // fallback: используем атомики (hot-path, более точная и лёгкая в runtime)
        let loads: Vec<u64> = (0..self.shard_count)
            .map(|i| self.shard_ops[i].load(Ordering::Relaxed))
            .collect();

        if loads.len() < 2 {
            return false;
        }
        if loads.iter().all(|&x| x == 0) {
            return false;
        }
        let max_load = *loads.iter().max().unwrap() as f64;
        let min_load = *loads.iter().min().unwrap() as f64;
        if min_load == 0.0 {
            return max_load > 0.0;
        }
        (max_load / min_load) > REBALANCE_THRESHOLD
    }

    /// Планирует (и ставит в очередь) миграции слотов для ребалансинга.
    ///
    /// Возвращает список спланированных пар (slot, from_shard, to_shard).
    ///
    /// Алгоритм:
    /// 1. Сначала пытается использовать агрегированные `load_metrics` (если они
    ///    непустые).
    /// 2. Если агрегированных метрик нет — использует атомики `shard_ops` и
    ///    `slot_access`.
    ///
    /// Примечание: реальное копирование данных между шардами не реализовано
    /// здесь — этот модуль только планирует и отслеживает состояние слотов
    /// и задач миграции.
    pub fn trigger_rebalance(&self) -> Vec<(SlotId, ShardId, ShardId)> {
        // Предпочитаем агрегированные метрики, если они есть (оператор / тесты /
        // агрегатор)
        let mut migrations: Vec<(SlotId, ShardId, ShardId)> = Vec::new();

        // 1) сначала пробуем load_metrics
        {
            let metrics = self.load_metrics.read().unwrap();
            if !metrics.operations_per_second.is_empty() {
                // reuse existing logic based on load_metrics
                let avg_load: f64 = metrics.operations_per_second.values().sum::<u64>() as f64
                    / metrics.operations_per_second.len() as f64;

                let mut overloaded_shards: Vec<(ShardId, u64)> = Vec::new();
                let mut underloaded_shards: Vec<(ShardId, u64)> = Vec::new();

                for (&shard_id, &load) in &metrics.operations_per_second {
                    if (load as f64) > avg_load * REBALANCE_THRESHOLD {
                        overloaded_shards.push((shard_id, load));
                    } else if (load as f64) < avg_load / REBALANCE_THRESHOLD {
                        underloaded_shards.push((shard_id, load));
                    }
                }

                overloaded_shards.sort_by(|a, b| b.1.cmp(&a.1));
                underloaded_shards.sort_by(|a, b| a.1.cmp(&b.1));

                let assignments = self.slot_assignments.read().unwrap();

                for (overloaded_shard, _) in overloaded_shards {
                    for (underloaded_shard, _) in &underloaded_shards {
                        let slots_to_migrate: Vec<SlotId> = assignments
                            .iter()
                            .enumerate()
                            .filter_map(|(idx, state)| {
                                if let SlotState::Stable(shard_id) = state {
                                    if *shard_id == overloaded_shard {
                                        if let Some(&access_count) =
                                            metrics.slot_access_count.get(&(idx as SlotId))
                                        {
                                            if access_count > HOT_KEY_THRESHOLD {
                                                return Some(idx as SlotId);
                                            }
                                        }
                                    }
                                }
                                None
                            })
                            .take(MIGRATION_BATCH_SIZE)
                            .collect();

                        for slot in slots_to_migrate {
                            migrations.push((slot, overloaded_shard, *underloaded_shard));
                        }

                        if migrations.len() >= MIGRATION_BATCH_SIZE {
                            break;
                        }
                    }
                    if migrations.len() >= MIGRATION_BATCH_SIZE {
                        break;
                    }
                }

                if !migrations.is_empty() {
                    let mut queue = self.migration_queue.lock().unwrap();
                    for m in &migrations {
                        queue.push_back(*m);
                    }
                    *self.last_rebalance.lock().unwrap() = Instant::now();
                    return migrations;
                }
            }
        }

        // 2) использовать атомики как запасной вариант, если агрегированных метрик нет
        // Формируем нагрузку на шарды из атомиков
        let shard_loads: Vec<(ShardId, u64)> = (0..self.shard_count)
            .map(|i| (i, self.shard_ops[i].load(Ordering::Relaxed)))
            .collect();

        let total_shard_ops: u64 = shard_loads.iter().map(|(_, v)| *v).sum();
        if total_shard_ops == 0 {
            return migrations;
        }

        let avg_load = (total_shard_ops as f64) / (self.shard_count as f64);

        let mut overloaded: Vec<(ShardId, u64)> = shard_loads
            .iter()
            .cloned()
            .filter(|(_, load)| (*load as f64) > avg_load * REBALANCE_THRESHOLD)
            .collect();

        let mut underloaded: Vec<(ShardId, u64)> = shard_loads
            .iter()
            .cloned()
            .filter(|(_, load)| (*load as f64) < avg_load / REBALANCE_THRESHOLD)
            .collect();

        if overloaded.is_empty() || underloaded.is_empty() {
            return migrations;
        }

        overloaded.sort_by(|a, b| b.1.cmp(&a.1));
        underloaded.sort_by(|a, b| a.1.cmp(&b.1));

        let assignments = self.slot_assignments.read().unwrap();

        'outer: for (ov_shard, _) in overloaded.iter() {
            for (ud_shard, _) in underloaded.iter() {
                for (slot_idx, state) in assignments.iter().enumerate() {
                    if migrations.len() >= MIGRATION_BATCH_SIZE {
                        break 'outer;
                    }
                    if let SlotState::Stable(sid) = state {
                        if *sid == *ov_shard {
                            let access = self.slot_access[slot_idx].load(Ordering::Relaxed);
                            if access > HOT_KEY_THRESHOLD {
                                let slot: SlotId = slot_idx as SlotId;
                                migrations.push((slot, *ov_shard, *ud_shard));
                            }
                        }
                    }
                }
            }
        }

        if !migrations.is_empty() {
            let mut queue = self.migration_queue.lock().unwrap();
            for m in &migrations {
                queue.push_back(*m);
            }
            *self.last_rebalance.lock().unwrap() = Instant::now();
        }

        migrations
    }

    /// Создаёт задачу миграции: переводит слот в состояние `Migrating` и
    /// регистрирует задачу.
    ///
    /// Возвращает типизированную ошибку `SlotManagerError` через `SmResult`.
    pub fn start_slot_migration(
        &self,
        slot: SlotId,
        from_shard: ShardId,
        to_shard: ShardId,
    ) -> SmResult<()> {
        {
            let active_migrations = self.active_migrations.lock()?;
            if active_migrations.contains_key(&slot) {
                return Err(SlotManagerError::MigrationActive(slot));
            }
        }

        let idx = slot as usize;
        {
            let mut assignments = self.slot_assignments.write()?;
            if idx >= assignments.len() {
                return Err(SlotManagerError::InvalidSlot(slot));
            }
            assignments[idx] = SlotState::Migrating {
                from: from_shard,
                to: to_shard,
                progress: 0.0,
            };
        }

        let migration_task = MigrationTask {
            slot,
            from_shard,
            to_shard,
            keys_to_migrate: Vec::new(),
            migrated_keys: 0,
            total_keys: 0,
            started_at: Instant::now(),
        };

        {
            let mut active_migrations = self.active_migrations.lock()?;
            active_migrations.insert(slot, migration_task);
        }

        self.slot_map_version.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    /// Завершает миграцию: переводит слот в Stable(to_shard) и удаляет задачу.
    pub fn complete_slot_migration(
        &self,
        slot: SlotId,
    ) -> SmResult<()> {
        let to_shard = {
            let active_migrations = self.active_migrations.lock()?;
            let migration = active_migrations
                .get(&slot)
                .ok_or(SlotManagerError::NoActiveMigration(slot))?;
            migration.to_shard
        };

        let idx = slot as usize;
        {
            let mut assignments = self.slot_assignments.write()?;
            if idx >= assignments.len() {
                return Err(SlotManagerError::InvalidSlot(slot));
            }
            assignments[idx] = SlotState::Stable(to_shard);
        }

        {
            let mut active_migrations = self.active_migrations.lock()?;
            active_migrations.remove(&slot);
        }

        self.slot_map_version.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    /// Откатывает миграцию — возвращает слот в `Stable(from_shard)`.
    pub fn rollback_migration(
        &self,
        slot: SlotId,
    ) -> SmResult<()> {
        let from_shard = {
            let active_migrations = self.active_migrations.lock()?;
            let migration = active_migrations
                .get(&slot)
                .ok_or(SlotManagerError::NoActiveMigration(slot))?;
            migration.from_shard
        };

        let idx = slot as usize;
        {
            let mut assignments = self.slot_assignments.write()?;
            if idx >= assignments.len() {
                return Err(SlotManagerError::InvalidSlot(slot));
            }
            assignments[idx] = SlotState::Stable(from_shard);
        }

        {
            let mut active_migrations = self.active_migrations.lock()?;
            active_migrations.remove(&slot);
        }

        self.slot_map_version.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    /// Возвращает список горячих ключей (по агрегированным метрикам).
    pub fn get_hot_keys(&self) -> Vec<(String, u64)> {
        let metrics = self.load_metrics.read().unwrap();
        let mut hot_keys: Vec<_> = metrics
            .hot_keys
            .iter()
            .filter(|(_, &count)| count > HOT_KEY_THRESHOLD)
            .map(|(k, &c)| (k.clone(), c))
            .collect();

        hot_keys.sort_by(|a, b| b.1.cmp(&a.1));
        hot_keys
    }

    /// Вероятностное распределение нагрузки по шардам (использует атомики).
    pub fn get_load_distribution(&self) -> HashMap<ShardId, f64> {
        let mut distribution = HashMap::new();
        let mut total = 0u64;
        for i in 0..self.shard_count {
            let v = self.shard_ops[i].load(Ordering::Relaxed);
            total += v;
            distribution.insert(i, v);
        }

        if total == 0 {
            return HashMap::new();
        }

        distribution
            .into_iter()
            .map(|(sid, ops)| (sid, ops as f64 / total as f64))
            .collect()
    }

    /// Информация о прогрессе активных миграций.
    pub fn get_migration_status(&self) -> Vec<(SlotId, ShardId, ShardId, f64)> {
        let active_migrations = self.active_migrations.lock().unwrap();
        active_migrations
            .values()
            .map(|task| {
                let progress = if task.total_keys > 0 {
                    task.migrated_keys as f64 / task.total_keys as f64
                } else {
                    0.0
                };
                (task.slot, task.from_shard, task.to_shard, progress)
            })
            .collect()
    }

    /// Сброс метрик (и атомиков).
    pub fn reset_metrics(&self) {
        // Reset aggregated load_metrics
        let mut metrics = self.load_metrics.write().unwrap();
        metrics.operations_per_second.clear();
        metrics.hot_keys.clear();
        metrics.slot_access_count.clear();
        metrics.last_updated = Instant::now();

        for a in self.shard_ops.iter() {
            a.store(0, Ordering::Relaxed);
        }
        for s in self.slot_access.iter() {
            s.store(0, Ordering::Relaxed);
        }
    }

    pub fn get_slot_map_version(&self) -> u64 {
        self.slot_map_version.load(Ordering::SeqCst)
    }
}

/// CRC16 (табличная реализация) совместимая с Redis.
/// Используется для вычисления слота (crc16(key) % 16384).
fn crc16(data: &[u8]) -> u16 {
    const CRC16_TAB: [u16; 256] = [
        0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7, 0x8108, 0x9129, 0xa14a,
        0xb16b, 0xc18c, 0xd1ad, 0xe1ce, 0xf1ef, 0x1231, 0x0210, 0x3273, 0x2252, 0x52b5, 0x4294,
        0x72f7, 0x62d6, 0x9339, 0x8318, 0xb37b, 0xa35a, 0xd3bd, 0xc39c, 0xf3ff, 0xe3de, 0x2462,
        0x3443, 0x0420, 0x1401, 0x64e6, 0x74c7, 0x44a4, 0x5485, 0xa56a, 0xb54b, 0x8528, 0x9509,
        0xe5ee, 0xf5cf, 0xc5ac, 0xd58d, 0x3653, 0x2672, 0x1611, 0x0630, 0x76d7, 0x66f6, 0x5695,
        0x46b4, 0xb75b, 0xa77a, 0x9719, 0x8738, 0xf7df, 0xe7fe, 0xd79d, 0xc7bc, 0x48c4, 0x58e5,
        0x6886, 0x78a7, 0x0840, 0x1861, 0x2802, 0x3823, 0xc9cc, 0xd9ed, 0xe98e, 0xf9af, 0x8948,
        0x9969, 0xa90a, 0xb92b, 0x5af5, 0x4ad4, 0x7ab7, 0x6a96, 0x1a71, 0x0a50, 0x3a33, 0x2a12,
        0xdbfd, 0xcbdc, 0xfbbf, 0xeb9e, 0x9b79, 0x8b58, 0xbb3b, 0xab1a, 0x6ca6, 0x7c87, 0x4ce4,
        0x5cc5, 0x2c22, 0x3c03, 0x0c60, 0x1c41, 0xedae, 0xfd8f, 0xcdec, 0xddcd, 0xad2a, 0xbd0b,
        0x8d68, 0x9d49, 0x7e97, 0x6eb6, 0x5ed5, 0x4ef4, 0x3e13, 0x2e32, 0x1e51, 0x0e70, 0xff9f,
        0xefbe, 0xdfdd, 0xcffc, 0xbf1b, 0xaf3a, 0x9f59, 0x8f78, 0x9188, 0x81a9, 0xb1ca, 0xa1eb,
        0xd10c, 0xc12d, 0xf14e, 0xe16f, 0x1080, 0x00a1, 0x30c2, 0x20e3, 0x5004, 0x4025, 0x7046,
        0x6067, 0x83b9, 0x9398, 0xa3fb, 0xb3da, 0xc33d, 0xd31c, 0xe37f, 0xf35e, 0x02b1, 0x1290,
        0x22f3, 0x32d2, 0x4235, 0x5214, 0x6277, 0x7256, 0xb5ea, 0xa5cb, 0x95a8, 0x8589, 0xf56e,
        0xe54f, 0xd52c, 0xc50d, 0x34e2, 0x24c3, 0x14a0, 0x0481, 0x7466, 0x6447, 0x5424, 0x4405,
        0xa7db, 0xb7fa, 0x8799, 0x97b8, 0xe75f, 0xf77e, 0xc71d, 0xd73c, 0x26d3, 0x36f2, 0x0691,
        0x16b0, 0x6657, 0x7676, 0x4615, 0x5634, 0xd94c, 0xc96d, 0xf90e, 0xe92f, 0x99c8, 0x89e9,
        0xb98a, 0xa9ab, 0x5844, 0x4865, 0x7806, 0x6827, 0x18c0, 0x08e1, 0x3882, 0x28a3, 0xcb7d,
        0xdb5c, 0xeb3f, 0xfb1e, 0x8bf9, 0x9bd8, 0xabbb, 0xbb9a, 0x4a75, 0x5a54, 0x6a37, 0x7a16,
        0x0af1, 0x1ad0, 0x2ab3, 0x3a92, 0xfd2e, 0xed0f, 0xdd6c, 0xcd4d, 0xbdaa, 0xad8b, 0x9de8,
        0x8dc9, 0x7c26, 0x6c07, 0x5c64, 0x4c45, 0x3ca2, 0x2c83, 0x1ce0, 0x0cc1, 0xef1f, 0xff3e,
        0xcf5d, 0xdf7c, 0xaf9b, 0xbfba, 0x8fd9, 0x9ff8, 0x6e17, 0x7e36, 0x4e55, 0x5e74, 0x2e93,
        0x3eb2, 0x0ed1, 0x1ef0,
    ];

    let mut crc = 0u16;
    for &byte in data {
        crc = (crc << 8) ^ CRC16_TAB[((crc >> 8) ^ byte as u16) as usize];
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет корректность вычисления слота:
    /// - обычный ключ должен давать значение в диапазоне TOTAL_SLOTS;
    /// - поддержка hash-tag `{...}`: два ключа с одинаковым тегом должны
    ///   попадать в один и тот же слот.
    #[test]
    fn test_slot_calculation() {
        let manager = SlotManager::new(3);
        let slot = manager.calculate_slot("mykey");
        assert!(slot < TOTAL_SLOTS);

        let slot1 = manager.calculate_slot("user:{123}:profile");
        let slot2 = manager.calculate_slot("user:{123}:settings");
        assert_eq!(slot1, slot2); // Should map to same slot due to hash tag
    }

    /// Тест проверяет поведение триггера ребалансинга `should_rebalance()`:
    /// когда агрегированные метрики (`load_metrics.operations_per_second`)
    /// содержат сильно неравномерную нагрузку (1000 vs 100), метод должен
    /// вернуть `true`.
    #[test]
    fn test_rebalance_trigger() {
        let manager = SlotManager::new(2);

        {
            let mut metrics = manager.load_metrics.write().unwrap();
            metrics.operations_per_second.insert(0, 1000);
            metrics.operations_per_second.insert(1, 100);
        }

        assert!(manager.should_rebalance());
    }

    /// Тест проверяет корректный workflow миграции слота:
    /// - `start_slot_migration` переводит слот в состояние `Migrating` и
    ///   регистрирует задачу;
    /// - `complete_slot_migration` завершает миграцию и переводит слот в
    ///   `Stable(to_shard)`.
    #[test]
    fn test_migration_workflow() {
        let manager = SlotManager::new(3);
        let slot: SlotId = 100;

        assert!(manager.start_slot_migration(slot, 0, 1).is_ok());

        let assignments = manager.slot_assignments.read().unwrap();
        match &assignments[slot as usize] {
            SlotState::Migrating { from, to, .. } => {
                assert_eq!(*from, 0);
                assert_eq!(*to, 1);
            }
            _ => panic!("Slot should be in migrating state"),
        }

        drop(assignments);
        assert!(manager.complete_slot_migration(slot).is_ok());
        assert_eq!(manager.get_slot_shard(slot), Some(1));
    }
}
