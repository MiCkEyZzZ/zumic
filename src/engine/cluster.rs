use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::{Duration, Instant},
};

use crate::{
    engine::slot_manager::{ShardId, SlotManager},
    GeoPoint, Sds, Storage, StoreError, StoreResult, Value,
};

/// `InClusterStore` — распределённое key-value хранилище,
/// объединяющее несколько shard'ов (`Storage`) под управлением
/// `SlotManager`.
pub struct InClusterStore {
    /// Список всех shard'ов (каждый реализует `Storage`).
    shards: Vec<Arc<dyn Storage>>,
    /// Менеджер распределения ключей по shard'ам.
    slot_manager: Arc<SlotManager>,
    /// Хэндл фонового треда-ребалансера.
    rebalancer_handle: Option<thread::JoinHandle<()>>,
    /// Флаг остановки фонового ребалансера.
    shutdown_flag: Arc<Mutex<bool>>,
    /// Примитивные метрики операций по кластеру.
    operation_metrics: Arc<RwLock<OperationMetrics>>,
}

/// Метрики операций в кластере.
///
/// Используются для тестов и базового мониторинга.
#[derive(Debug, Clone)]
pub struct OperationMetrics {
    /// Общее количество операций (set/get/del/...).
    pub total_operations: u64,
    /// Количество операций, затронувших более одного shard'а.
    pub cross_shard_operations: u64,
    /// Количество миграций слотов.
    pub migration_operations: u64,
    /// Количество неудачных операций (ошибки маршрутизации и т.п.).
    pub failed_operations: u64,
    /// Среднее время ответа (мс). Пока вычисляется упрощённо.
    pub average_response_time_ms: f64,
    /// Время последнего сброса метрик.
    pub last_reset: Instant,
}

impl InClusterStore {
    /// Создаёт новый кластер из списка shard'ов.
    ///
    /// - каждый shard обязан реализовывать `Storage`,
    /// - создаётся `SlotManager` по количеству shard'ов,
    /// - запускается фоновый тред ребалансировки.
    pub fn new(shards: Vec<Arc<dyn Storage>>) -> Self {
        let shard_count = shards.len();
        let slot_manager = Arc::new(SlotManager::new(shard_count));
        let shutdown_flag = Arc::new(Mutex::new(false));

        let rebalancer_handle = Self::start_rebalancer(slot_manager.clone(), shutdown_flag.clone());

        Self {
            shards,
            slot_manager,
            rebalancer_handle: Some(rebalancer_handle),
            shutdown_flag,
            operation_metrics: Arc::new(RwLock::new(OperationMetrics {
                last_reset: Instant::now(),
                ..Default::default()
            })),
        }
    }

    /// Создаёт кластер с кастомным `SlotManager`.
    ///
    /// Полезно в тестах или при явном контроле распределения ключей.
    pub fn new_with_slot_manager(
        shards: Vec<Arc<dyn Storage>>,
        slot_manager: Arc<SlotManager>,
    ) -> Self {
        let shutdown_flag = Arc::new(Mutex::new(false));
        let rebalancer_handle = Self::start_rebalancer(slot_manager.clone(), shutdown_flag.clone());

        Self {
            shards,
            slot_manager,
            rebalancer_handle: Some(rebalancer_handle),
            shutdown_flag,
            operation_metrics: Arc::new(RwLock::new(OperationMetrics {
                last_reset: Instant::now(),
                ..Default::default()
            })),
        }
    }

    /// Фоновый процесс, периодически проверяющий необходимость
    /// ребалансировки и инициирующий миграции слотов.
    fn start_rebalancer(
        slot_manager: Arc<SlotManager>,
        shutdown_flag: Arc<Mutex<bool>>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut last_check = Instant::now();
            let check_interval = Duration::from_secs(10);

            loop {
                // shutdown?
                {
                    let shutdown = *shutdown_flag.lock().unwrap();
                    if shutdown {
                        break;
                    }
                }

                if last_check.elapsed() >= check_interval {
                    if slot_manager.should_rebalance() {
                        let migrations = slot_manager.trigger_rebalance();

                        for (slot, from_shard, to_shard) in migrations {
                            if let Err(e) =
                                slot_manager.start_slot_migration(slot, from_shard, to_shard)
                            {
                                eprintln!("Failed to start migration for slot {slot}: {e:?}");
                            } else {
                                println!(
                                    "Started migration: slot {slot} from shard {from_shard} to shard {to_shard}"
                                );
                            }
                        }
                    }
                    last_check = Instant::now();
                }

                thread::sleep(Duration::from_millis(100));
            }
        })
    }

    /// Получить shard по его идентификатору.
    ///
    /// Возвращает ошибку `WrongShard`, если индекс некорректный.
    fn shard_by_id(
        &self,
        shard_id: ShardId,
    ) -> Result<Arc<dyn Storage>, StoreError> {
        match self.shards.get(shard_id).cloned() {
            Some(s) => Ok(s),
            None => {
                self.record_failed_operation();
                Err(StoreError::WrongShard)
            }
        }
    }

    /// Утилита: преобразовать `Sds` в `&str` (lossy).
    fn sds_to_str<'a>(s: &'a Sds) -> std::borrow::Cow<'a, str> {
        std::string::String::from_utf8_lossy(s.as_bytes())
    }

    /// Учёт обычной операции (увеличение счётчика + запись в slot_manager).
    fn record_operation(
        &self,
        key: &Sds,
    ) {
        let ks = Self::sds_to_str(key);
        self.slot_manager.record_operation(ks.as_ref());
        let mut m = self.operation_metrics.write().unwrap();
        m.total_operations += 1;
    }

    /// Учёт cross-shard операции.
    fn record_cross_shard_operation(&self) {
        let mut m = self.operation_metrics.write().unwrap();
        m.cross_shard_operations += 1;
    }

    /// Учёт неудачной операции.
    fn record_failed_operation(&self) {
        let mut m = self.operation_metrics.write().unwrap();
        m.failed_operations += 1;
    }

    /// Доступ к `SlotManager` (например, для тестов или мониторинга).
    pub fn get_slot_manager(&self) -> &Arc<SlotManager> {
        &self.slot_manager
    }
}

impl Storage for InClusterStore {
    fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.set(key, value)
    }

    fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.get(key)
    }

    fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.del(key)
    }

    fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut groups: HashMap<ShardId, Vec<(&Sds, Value)>> = HashMap::new();
        for (k, v) in entries {
            let ks = Self::sds_to_str(k);
            let shard_id = self.slot_manager.get_key_shard(ks.as_ref());
            groups.entry(shard_id).or_default().push((k, v));
            self.slot_manager.record_operation(ks.as_ref());
        }

        let groups_count = groups.len();

        for (shard_id, vec) in groups.into_iter() {
            let shard = self.shard_by_id(shard_id)?;
            shard.mset(vec)?;
        }

        if groups_count > 1 {
            self.record_cross_shard_operation();
            // Временно для проверки работы
            let metrics = self.operation_metrics.read().unwrap();
            println!(
                "Cross-shard operations after MSET: {}",
                metrics.cross_shard_operations
            );
        }

        Ok(())
    }

    fn mget(
        &self,
        keys: &[&Sds],
    ) -> StoreResult<Vec<Option<Value>>> {
        if keys.is_empty() {
            return Ok(vec![]);
        }

        let mut groups: HashMap<ShardId, Vec<(usize, &Sds)>> = HashMap::new();
        for (i, &k) in keys.iter().enumerate() {
            let ks = Self::sds_to_str(k);
            let shard_id = self.slot_manager.get_key_shard(ks.as_ref());
            groups.entry(shard_id).or_default().push((i, k));
            self.slot_manager.record_operation(ks.as_ref());
        }

        let mut results: Vec<Option<Value>> = vec![None; keys.len()];

        for (shard_id, list) in groups.iter() {
            let shard = self.shard_by_id(*shard_id)?;
            let shard_keys: Vec<&Sds> = list.iter().map(|(_, k)| *k).collect();
            let shard_results = shard.mget(&shard_keys)?;
            for ((idx, _), res) in list.iter().zip(shard_results.into_iter()) {
                results[*idx] = res;
            }
        }

        if groups.len() > 1 {
            self.record_cross_shard_operation();
            // Временно для проверки работы
            let metrics = self.operation_metrics.read().unwrap();
            println!(
                "Cross-shard operations after MGET: {}",
                metrics.cross_shard_operations
            );
        }

        Ok(results)
    }

    fn rename(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<()> {
        let from_str = Self::sds_to_str(from);
        let to_str = Self::sds_to_str(to);

        let from_shard = self.slot_manager.get_key_shard(from_str.as_ref());
        let to_shard = self.slot_manager.get_key_shard(to_str.as_ref());

        self.slot_manager.record_operation(from_str.as_ref());
        self.slot_manager.record_operation(to_str.as_ref());

        if from_shard != to_shard {
            return Err(StoreError::WrongShard);
        }

        let shard = self.shard_by_id(from_shard)?;
        shard.rename(from, to)
    }

    fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool> {
        let from_str = Self::sds_to_str(from);
        let to_str = Self::sds_to_str(to);

        let from_shard = self.slot_manager.get_key_shard(from_str.as_ref());
        let to_shard = self.slot_manager.get_key_shard(to_str.as_ref());

        self.slot_manager.record_operation(from_str.as_ref());
        self.slot_manager.record_operation(to_str.as_ref());

        if from_shard != to_shard {
            return Err(StoreError::WrongShard);
        }

        let shard = self.shard_by_id(from_shard)?;
        shard.renamenx(from, to)
    }

    fn flushdb(&self) -> StoreResult<()> {
        for shard in &self.shards {
            shard.flushdb()?;
        }
        self.slot_manager.reset_metrics();
        Ok(())
    }

    fn geo_add(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        member: &Sds,
    ) -> StoreResult<bool> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.geo_add(key, lon, lat, member)
    }

    fn geo_dist(
        &self,
        key: &Sds,
        member1: &Sds,
        member2: &Sds,
        unit: &str,
    ) -> StoreResult<Option<f64>> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.geo_dist(key, member1, member2, unit)
    }

    fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.geo_pos(key, member)
    }

    fn geo_radius(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.geo_radius(key, lon, lat, radius, unit)
    }

    fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        self.record_operation(key);
        let key_str = Self::sds_to_str(key);
        let shard_id = self.slot_manager.get_key_shard(key_str.as_ref());
        let shard = self.shard_by_id(shard_id)?;
        shard.geo_radius_by_member(key, member, radius, unit)
    }
}

impl Drop for InClusterStore {
    /// При уничтожении объекта останавливает фоновый тред ребалансера.
    fn drop(&mut self) {
        // Signal shutdown to background thread
        {
            let mut shutdown = self.shutdown_flag.lock().unwrap();
            *shutdown = true;
        }

        // Wait for background thread to finish
        if let Some(handle) = self.rebalancer_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Default for OperationMetrics {
    fn default() -> Self {
        Self {
            total_operations: 0,
            cross_shard_operations: 0,
            migration_operations: 0,
            failed_operations: 0,
            average_response_time_ms: 0.0,
            last_reset: Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryStore, Sds, Value};

    #[allow(clippy::arc_with_non_send_sync)]
    fn make_cluster(shards: usize) -> InClusterStore {
        let vec = (0..shards)
            .map(|_| Arc::new(InMemoryStore::new()) as Arc<dyn Storage>)
            .collect();
        InClusterStore::new(vec)
    }

    #[test]
    fn test_set_get_del() {
        let cluster = make_cluster(2);

        let key = Sds::from_str("foo");
        let val = Value::Str(Sds::from_str("bar"));

        cluster.set(&key, val.clone()).unwrap();
        assert_eq!(cluster.get(&key).unwrap(), Some(val.clone()));

        assert!(cluster.del(&key).unwrap());
        assert_eq!(cluster.get(&key).unwrap(), None);
    }

    #[test]
    fn test_mset_mget_cross_shard() {
        let cluster = make_cluster(4);

        let k1 = Sds::from_str("key1");
        let k2 = Sds::from_str("key2");
        let v1 = Value::Str(Sds::from_str("v1"));
        let v2 = Value::Str(Sds::from_str("v2"));

        cluster
            .mset(vec![(&k1, v1.clone()), (&k2, v2.clone())])
            .unwrap();
        let res = cluster.mget(&[&k1, &k2]).unwrap();

        assert_eq!(res, vec![Some(v1), Some(v2)]);

        // Проверяем, что cross_shard учёлся
        let metrics = cluster.operation_metrics.read().unwrap().clone();
        assert!(metrics.cross_shard_operations >= 1);
    }

    #[test]
    fn test_rename_and_renamenx() {
        let cluster = make_cluster(1);

        let k1 = Sds::from_str("a");
        let k2 = Sds::from_str("b");

        cluster.set(&k1, Value::Str(Sds::from_str("v"))).unwrap();

        cluster.rename(&k1, &k2).unwrap();
        assert_eq!(
            cluster.get(&k2).unwrap(),
            Some(Value::Str(Sds::from_str("v")))
        );
        assert_eq!(cluster.get(&k1).unwrap(), None);

        let k3 = Sds::from_str("c");
        cluster.set(&k3, Value::Str(Sds::from_str("c"))).unwrap();
        let res = cluster.renamenx(&k2, &k3);
        assert!(res.is_err() || !res.unwrap());
    }

    #[test]
    fn test_flushdb() {
        let cluster = make_cluster(3);
        let key = Sds::from_str("flushme");

        cluster
            .set(&key, Value::Str(Sds::from_str("data")))
            .unwrap();
        assert_eq!(
            cluster.get(&key).unwrap(),
            Some(Value::Str(Sds::from_str("data")))
        );

        cluster.flushdb().unwrap();
        assert_eq!(cluster.get(&key).unwrap(), None);
    }

    #[test]
    fn test_geo_ops() {
        let cluster = make_cluster(2);
        let key = Sds::from_str("geo");
        let member = Sds::from_str("rome");

        assert!(cluster.geo_add(&key, 12.5, 41.9, &member).unwrap());
        let pos = cluster.geo_pos(&key, &member).unwrap().unwrap();
        assert!((pos.lon - 12.5).abs() < 1e-6);
        assert!((pos.lat - 41.9).abs() < 1e-6);

        let member2 = Sds::from_str("milan");
        cluster.geo_add(&key, 9.19, 45.46, &member2).unwrap();

        let dist = cluster
            .geo_dist(&key, &member, &member2, "km")
            .unwrap()
            .unwrap();
        assert!(dist > 400.0); // реальное расстояние ~ 480км
    }
}
