use std::{
    collections::HashMap,
    io::Cursor,
    path::Path,
    sync::{atomic::Ordering, Mutex},
};

use super::{
    aof::{AofOp, SyncPolicy},
    write_stream, AofLog, Storage, StreamReader,
};
use crate::{
    GeoPoint, GeoSet, GlobalShardStats, Sds, ShardMetricsSnapshot, ShardedIndex, ShardingConfig,
    StoreError, StoreResult, Value,
};

/// Конфигурация для InPersistentStore
#[derive(Debug, Clone)]
pub struct PersistentStoreConfig {
    /// Конфигурация шардирования
    pub sharding: ShardingConfig,
    /// Политика синхронизации
    pub sync_policy: SyncPolicy,
    /// Включить детальное логирование операций
    pub enable_operation_logging: bool,
}

/// Хранилище с поддержкой постоянства через AOF и sharded индекс.
/// Ключи и значения распределены по шардам, изменения логируются на диск.
pub struct InPersistentStore {
    /// Sharded in-memory индекс для concurrent access
    index: ShardedIndex<Vec<u8>>,
    /// Журнал AOF, логирующий изменения (один для всех шардов)
    aof: Mutex<AofLog>,
    /// Конфигурация хранилища
    #[allow(dead_code)]
    config: PersistentStoreConfig,
}

impl InPersistentStore {
    /// Создаёт новое хранилище с журналом AOF и sharded индексом.
    /// При инициализации восстанавливает состояние из AOF.
    pub fn new<P: AsRef<Path>>(
        path: P,
        config: PersistentStoreConfig,
    ) -> Result<Self, StoreError> {
        let aof = AofLog::open(path, config.sync_policy)?;
        let index = ShardedIndex::new(config.sharding.clone());

        let store = Self {
            index,
            aof: Mutex::new(aof),
            config,
        };

        // Восстанавливаем состояние из AOF
        store.replay_aof()?;

        Ok(store)
    }

    /// Возвращает информацию о шардировании.
    pub fn sharding_info(&self) -> (usize, GlobalShardStats) {
        (self.index.num_shards(), self.index.global_stats())
    }

    /// Возвращает метрики по всем шардам (для мониторинга).
    pub fn get_shard_metrics(&self) -> Vec<ShardMetricsSnapshot> {
        self.index.collect_metrics()
    }

    /// Восстанавливает состояние из AOF журнала.
    fn replay_aof(&self) -> StoreResult<()> {
        let mut aof_guard = self.aof.lock().unwrap();

        aof_guard.replay(|op, key, val| {
            let shard = self.index.get_shard(&key);

            shard.write(|data| {
                match op {
                    AofOp::Set => {
                        if let Some(value) = val {
                            let was_new = !data.contains_key(&key);
                            data.insert(key, value);
                            // Обновляем метрики только для новых ключей
                            if was_new {
                                if let Some(ref metrics) = shard.metrics {
                                    metrics.increment_key_count();
                                }
                            }
                        }
                    }
                    AofOp::Del => {
                        if data.remove(&key).is_some() {
                            if let Some(ref metrics) = shard.metrics {
                                metrics.decrement_key_count();
                            }
                        }
                    }
                }
            });
        })?;

        Ok(())
    }

    /// Блокирует два шарда для записи (в порядке id) и выполняет замыкание,
    /// передавая mutable reference на их HashMap'ы.
    fn with_two_shards_write<F, R>(
        &self,
        a: usize,
        b: usize,
        mut f: F,
    ) -> R
    where
        F: FnMut(
            &mut std::collections::HashMap<Vec<u8>, Vec<u8>>,
            &mut std::collections::HashMap<Vec<u8>, Vec<u8>>,
        ) -> R,
    {
        let shards = self.index.all_shards();
        let (first_id, second_id) = if a <= b { (a, b) } else { (b, a) };
        let first = &shards[first_id];
        let second = &shards[second_id];

        // Берём write guard один раз для каждого шарда
        let mut g1 = first.data.write().unwrap();
        let mut g2 = second.data.write().unwrap();

        if a == first_id {
            f(&mut g1, &mut g2)
        } else {
            f(&mut g2, &mut g1)
        }
    }
}

impl Storage for InPersistentStore {
    /// Устанавливает значение по ключу, логируя операцию в AOF.
    fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()> {
        let key_b = key.as_bytes();
        let val_b = value.to_bytes();

        // Логируем в AOF (синхронно для консистентности)
        {
            let mut aof = self.aof.lock().unwrap();
            aof.append_set(key_b, &val_b)?;
        }

        // Записываем в соответствующий шард
        let shard = self.index.get_shard(key_b);
        shard.write(|data| {
            let was_new = !data.contains_key(key_b);
            data.insert(key_b.to_vec(), val_b);

            // Обновляем key count метрику
            if was_new {
                if let Some(ref metrics) = shard.metrics {
                    metrics.increment_key_count();
                }
            }
        });

        Ok(())
    }

    /// Получает значение по ключу, если оно существует.
    fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>> {
        let key_b = key.as_bytes();
        let shard = self.index.get_shard(key_b);

        shard.read(|data| match data.get(key_b) {
            Some(val) => Ok(Some(Value::from_bytes(val)?)),
            None => Ok(None),
        })
    }

    /// Удаляет ключ, если он есть, и логирует удаление в AOF.
    fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool> {
        let key_b = key.as_bytes();

        // Проверяем существование ключа и удаляем из шрда
        let shard = self.index.get_shard(key_b);
        let existed = shard.write(|data| {
            if data.remove(key_b).is_some() {
                if let Some(ref metrics) = shard.metrics {
                    metrics.decrement_key_count();
                }
                true
            } else {
                false
            }
        });

        // Логируем в AOF только если ключ действительно был удалён
        if existed {
            let mut aof = self.aof.lock().unwrap();
            aof.append_del(key_b)?;
        }

        Ok(existed)
    }

    /// Устанавливает несколько пар ключ-значение сразу.
    /// Оптимизирован для минимизации cross-shard locks.
    fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()> {
        // map key_bytes -> serialized value bytes
        let mut kv_lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::with_capacity(entries.len());
        for (k, v) in &entries {
            kv_lookup.insert(k.as_bytes().to_vec(), v.to_bytes());
        }

        let keys_bytes: Vec<_> = entries.iter().map(|(k, _)| k.as_bytes()).collect();
        let groups = self.index.group_keys_by_shard(&keys_bytes);

        // WAL: логируем все операции
        {
            let mut aof = self.aof.lock().unwrap();
            for (k, v) in &kv_lookup {
                aof.append_set(k.as_slice(), v)?;
            }
        }

        // применяем изменения по шардам
        for (shard_id, shard_keys) in groups {
            let shard = &self.index.all_shards()[shard_id];
            shard.write(|data| {
                let mut new_keys = 0u64;
                for key_bytes in shard_keys {
                    if let Some(val_b) = kv_lookup.get(key_bytes) {
                        let was_new = !data.contains_key(key_bytes);
                        data.insert(key_bytes.to_vec(), val_b.clone());
                        if was_new {
                            new_keys += 1;
                        }
                    }
                }
                if new_keys > 0 {
                    if let Some(ref metrics) = shard.metrics {
                        // напрямую увеличиваем AtomicU64 пачкой
                        metrics.key_count.fetch_add(new_keys, Ordering::Relaxed);
                    }
                }
            });
        }

        Ok(())
    }

    /// Получает значения по списку ключей.
    /// Оптимизирован для минимизации cross-shard locks.
    fn mget(
        &self,
        keys: &[&Sds],
    ) -> StoreResult<Vec<Option<Value>>> {
        // Группируем ключи по шардам с исходными индексами
        let mut groups: HashMap<usize, Vec<(usize, &[u8])>> = HashMap::new();
        for (i, k) in keys.iter().enumerate() {
            let kb = k.as_bytes();
            let sid = self.index.shard_for_key(kb);
            groups.entry(sid).or_default().push((i, kb));
        }

        let mut result = vec![None; keys.len()];

        let mut shard_ids: Vec<usize> = groups.keys().cloned().collect();
        shard_ids.sort_unstable();

        for sid in shard_ids {
            let items = groups.remove(&sid).unwrap();
            let shard = &self.index.all_shards()[sid];

            let shard_results: Vec<Option<Value>> = shard.read(|data| {
                items
                    .iter()
                    .map(|&(_, key_bytes)| {
                        data.get(key_bytes)
                            .map(|val| Value::from_bytes(val))
                            .transpose()
                    })
                    .collect::<Result<Vec<_>, _>>()
            })?;

            for (j, &(orig_idx, _)) in items.iter().enumerate() {
                result[orig_idx] = shard_results[j].clone();
            }
        }

        Ok(result)
    }

    /// Переименовывает ключ, если он существует.
    /// Удаляет старый и добавляет новый, логируя оба действия.
    fn rename(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<()> {
        let from_b = from.as_bytes();
        let to_b = to.as_bytes();

        let from_shard_id = self.index.shard_for_key(from_b);
        let to_shard_id = self.index.shard_for_key(to_b);

        if from_shard_id == to_shard_id {
            // Простой случай: оба ключа в одном шарде
            let shard = &self.index.all_shards()[from_shard_id];
            let value = shard.write(|data| {
                if let Some(val) = data.remove(from_b) {
                    let to_was_new = !data.contains_key(to_b);
                    data.insert(to_b.to_vec(), val.clone());

                    // Обновляем метрики: удален один ключ, добавлен другой
                    if let Some(ref metrics) = shard.metrics {
                        metrics.decrement_key_count();
                        if to_was_new {
                            metrics.increment_key_count();
                        }
                    }

                    Some(val)
                } else {
                    None
                }
            });

            if let Some(val) = value {
                // Логируем обе операции в AOF
                let mut aof = self.aof.lock().unwrap();
                aof.append_del(from_b)?;
                aof.append_set(to_b, &val)?;
                Ok(())
            } else {
                Err(StoreError::KeyNotFound)
            }
        } else {
            // Cross-shard: делаем атомарную swap-перемещалку под двумя write-guards
            let val_res: StoreResult<Vec<u8>> =
                self.with_two_shards_write(from_shard_id, to_shard_id, |from_map, to_map| {
                    // from_map соответствует `from_shard_id`, to_map — `to_shard_id`
                    if !from_map.contains_key(from_b) {
                        return Err(StoreError::KeyNotFound);
                    }
                    let val = from_map.remove(from_b).unwrap();
                    let to_was_new = !to_map.contains_key(to_b);
                    to_map.insert(to_b.to_vec(), val.clone());

                    // обновляем метрики через index (atomic)
                    if let Some(ref m) = self.index.all_shards()[from_shard_id].metrics {
                        m.decrement_key_count();
                    }
                    if to_was_new {
                        if let Some(ref m) = self.index.all_shards()[to_shard_id].metrics {
                            m.increment_key_count();
                        }
                    }

                    Ok(val)
                });

            let value = val_res?; // если Err -> выйдем вверх

            // Логируем операции в AOF
            let mut aof = self.aof.lock().unwrap();
            aof.append_del(from_b)?;
            aof.append_set(to_b, &value)?;

            Ok(())
        }
    }

    /// Как `rename`, но не переименовывает, если целевой ключ уже существует.
    fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool> {
        let from_b = from.as_bytes();
        let to_b = to.as_bytes();

        let from_shard_id = self.index.shard_for_key(from_b);
        let to_shard_id = self.index.shard_for_key(to_b);

        if from_shard_id == to_shard_id {
            // Простой случай: оба ключа в одном шарде
            let shard = &self.index.all_shards()[from_shard_id];
            let result = shard.write(|data| {
                // Проверяем существование исходного ключа
                if !data.contains_key(from_b) {
                    return Err(StoreError::KeyNotFound);
                }

                // Проверяем, не существует ли целевой ключ
                if data.contains_key(to_b) {
                    return Ok(false);
                }

                // Выполняем перенос
                if let Some(val) = data.remove(from_b) {
                    data.insert(to_b.to_vec(), val.clone());
                    // Метрики не меняются: удалили один, добавили другой
                    Ok(true)
                } else {
                    // Теоретически unreachable после предыдущих проверок
                    Ok(false)
                }
            })?;

            if result {
                // Логируем операции только при успешном выполнении
                let mut aof = self.aof.lock().unwrap();
                aof.append_del(from_b)?;
                aof.append_set(to_b, shard.data.read().unwrap().get(to_b).unwrap())?;
            }

            Ok(result)
        } else {
            // Cross-shard case: делаем всю логику под двумя write-guards через helper
            let res: StoreResult<bool> =
                self.with_two_shards_write(from_shard_id, to_shard_id, |from_map, to_map| {
                    // Проверяем условия
                    if !from_map.contains_key(from_b) {
                        return Err(StoreError::KeyNotFound);
                    }
                    if to_map.contains_key(to_b) {
                        return Ok(false);
                    }

                    // Перенос
                    let val = from_map.remove(from_b).unwrap();
                    let to_was_new = !to_map.contains_key(to_b);
                    to_map.insert(to_b.to_vec(), val.clone());

                    // обновляем метрики
                    if let Some(ref m) = self.index.all_shards()[from_shard_id].metrics {
                        m.decrement_key_count();
                    }
                    if to_was_new {
                        if let Some(ref m) = self.index.all_shards()[to_shard_id].metrics {
                            m.increment_key_count();
                        }
                    }

                    Ok(true)
                });

            let performed = res?;

            if performed {
                // Логирование операции
                // Для aof нужно значение; его мы уже вставили — читаем его под read
                let value = {
                    let shard = &self.index.all_shards()[to_shard_id];
                    shard.read(|data| data.get(to_b).cloned().unwrap())
                };
                let mut aof = self.aof.lock().unwrap();
                aof.append_del(from_b)?;
                aof.append_set(to_b, &value)?;
            }

            Ok(performed)
        }
    }

    /// Очищает всё in-memory содержимое всех шардов.
    fn flushdb(&self) -> StoreResult<()> {
        for shard in self.index.all_shards().iter() {
            shard.write(|data| {
                let old_count = data.len() as u64;
                data.clear();

                if let Some(ref metrics) = shard.metrics {
                    metrics.key_count.fetch_sub(old_count, Ordering::Relaxed);
                }
            });
        }
        Ok(())
    }

    /// Добавляет точку (member, lon, lat) в гео-множество по ключу.
    /// Возвращает `true`, если member был добавлен впервые.
    fn geo_add(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        member: &Sds,
    ) -> StoreResult<bool> {
        let key_b = key.as_bytes();
        let shard = self.index.get_shard(key_b);

        let result: StoreResult<(Vec<u8>, bool)> = shard.write(|data| {
            // Восстановить предыдущий GeoSet
            let mut gs = if let Some(raw) = data.get(key_b) {
                let mut rdr =
                    StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
                let mut tmp = GeoSet::new();
                while let Some(Ok((m_sds, val))) = rdr.next() {
                    if let Value::Array(arr) = val {
                        if let [Value::Float(lon0), Value::Float(lat0)] = &arr[..] {
                            let m = m_sds.as_str()?;
                            tmp.add(m.to_string(), *lon0, *lat0);
                        }
                    }
                }
                tmp
            } else {
                GeoSet::new()
            };

            // Добавить новую точку
            let existed = gs.get(member.as_str()?).is_some();
            gs.add(member.to_string(), lon, lat);
            let added = !existed;

            // Сериализовать
            let mut buf = Vec::new();
            let entries = gs.entries.iter().map(|e| {
                let key = Sds::from_str(&e.member);
                let v = Value::Array(vec![Value::Float(e.point.lon), Value::Float(e.point.lat)]);
                (key, v)
            });
            write_stream(&mut buf, entries).map_err(StoreError::Io)?;

            let was_new_key = !data.contains_key(key_b);
            data.insert(key_b.to_vec(), buf.clone());

            if was_new_key {
                if let Some(ref metrics) = shard.metrics {
                    metrics.increment_key_count();
                }
            }

            Ok((buf, added))
        });

        let result = result?; // распаковываем StoreResult

        // Логируем в AOF
        {
            let mut aof = self.aof.lock().unwrap();
            aof.append_set(key_b, &result.0)?;
        }

        Ok(result.1)
    }

    /// Вычисляет расстояние между двумя членами множества в единицах `unit`.
    /// Если один из членов не найден, возвращает `None`.
    fn geo_dist(
        &self,
        key: &Sds,
        member1: &Sds,
        member2: &Sds,
        unit: &str,
    ) -> StoreResult<Option<f64>> {
        let key_b = key.as_bytes();
        let shard = self.index.get_shard(key_b);

        shard.read(|data| {
            let raw = match data.get(key_b) {
                Some(r) => r,
                None => return Ok(None),
            };

            // Восстановить GeoSet
            let mut gs = GeoSet::new();
            let mut rdr = StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
            while let Some(Ok((m_sds, val))) = rdr.next() {
                if let Value::Array(arr) = val {
                    if let [Value::Float(lon0), Value::Float(lat0)] = &arr[..] {
                        let m = m_sds.as_str()?;
                        gs.add(m.to_string(), *lon0, *lat0);
                    }
                }
            }

            // Конвертировать Sds → &str и посчитать дистанцию
            let m1 = member1.as_str()?;
            let m2 = member2.as_str()?;
            let meters = gs.dist(m1, m2);

            // Перевести в нужные единицы
            Ok(meters.map(|m| match unit {
                "km" => m / 1000.0,
                "mi" => m / 1609.344,
                "ft" => m * 3.28084,
                _ => m,
            }))
        })
    }

    /// Возвращает координаты `member` в GeoPoint, или `None`, если member не найден.
    fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>> {
        let key_b = key.as_bytes();
        let shard = self.index.get_shard(key_b);

        shard.read(|data| {
            let raw = match data.get(key_b) {
                Some(r) => r,
                None => return Ok(None),
            };

            let mut rdr = StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
            while let Some(Ok((m_sds, val))) = rdr.next() {
                if m_sds.as_str()? == member.as_str()? {
                    if let Value::Array(arr) = val {
                        if let [Value::Float(lon), Value::Float(lat)] = &arr[..] {
                            return Ok(Some(GeoPoint {
                                lon: *lon,
                                lat: *lat,
                            }));
                        }
                    }
                }
            }
            Ok(None)
        })
    }

    /// Находит всех членов в радиусе `radius` вокруг точки `(lon, lat)`.
    /// Возвращает вектор `(member, distance, GeoPoint)` в единицах `unit`.
    fn geo_radius(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        let key_b = key.as_bytes();
        let shard = self.index.get_shard(key_b);

        shard.read(|data| {
            let raw = match data.get(key_b) {
                Some(r) => r,
                None => return Ok(vec![]),
            };

            // Восстановить GeoSet
            let mut gs = GeoSet::new();
            let mut rdr = StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
            while let Some(Ok((m_sds, val))) = rdr.next() {
                if let Value::Array(arr) = val {
                    if let [Value::Float(lon0), Value::Float(lat0)] = &arr[..] {
                        let m = m_sds.as_str()?;
                        gs.add(m.to_string(), *lon0, *lat0);
                    }
                }
            }

            // Перевести radius в метры
            let r_m = match unit {
                "km" => radius * 1000.0,
                "mi" => radius * 1609.344,
                "ft" => radius / 3.28084,
                _ => radius,
            };

            // Сформировать результат
            let mut out = Vec::new();
            for (m, dist_m) in gs.radius(lon, lat, r_m) {
                let dist = match unit {
                    "km" => dist_m / 1000.0,
                    "mi" => dist_m / 1609.344,
                    "ft" => dist_m * 3.28084,
                    _ => dist_m,
                };
                let pt = gs.get(&m).unwrap();
                out.push((m, dist, pt));
            }
            Ok(out)
        })
    }

    /// Аналогично `geo_radius`, но центр задаётся существующим `member`.
    fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        // Сначала получаем позицию
        let center = self.geo_pos(key, member)?;
        if let Some(GeoPoint { lon, lat }) = center {
            self.geo_radius(key, lon, lat, radius, unit)
        } else {
            Ok(vec![])
        }
    }
}

impl Default for PersistentStoreConfig {
    fn default() -> Self {
        Self {
            sharding: ShardingConfig::default(),
            sync_policy: SyncPolicy::Always,
            enable_operation_logging: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::*;

    // Вспомогательная функция для создания хранилища с sharding
    fn new_sharded_store(num_shards: usize) -> Result<InPersistentStore, StoreError> {
        let temp_file = NamedTempFile::new()?;
        let config = PersistentStoreConfig {
            sharding: ShardingConfig {
                num_shards,
                enable_metrics: true,
                slow_operation_threshold_us: 1000,
            },
            sync_policy: SyncPolicy::Always,
            enable_operation_logging: false,
        };
        InPersistentStore::new(temp_file, config)
    }

    #[test]
    fn test_set_and_get() -> StoreResult<()> {
        let store = new_sharded_store(4)?;

        let key = Sds::from_str("test_key");
        let value = Value::Str(Sds::from_str("test_value"));

        store.set(&key, value.clone())?;
        let retrieved = store.get(&key)?;

        assert_eq!(retrieved, Some(value));

        // Проверяем, что ключ попал в правильный шард
        let stats = store.sharding_info().1;
        assert_eq!(stats.total_keys, 1);

        Ok(())
    }

    #[test]
    fn test_sharded_mset_mget() -> StoreResult<()> {
        let store = new_sharded_store(3)?;

        let keys: Vec<Sds> = (0..10)
            .map(|i| Sds::from_str(&format!("key_{}", i)))
            .collect();
        let values: Vec<Value> = (0..10)
            .map(|i| Value::Str(Sds::from_str(&format!("value_{}", i))))
            .collect();

        let entries: Vec<(&Sds, Value)> = keys.iter().zip(values.iter().cloned()).collect();

        store.mset(entries)?;

        let key_refs: Vec<&Sds> = keys.iter().collect();
        let retrieved = store.mget(&key_refs)?;

        assert_eq!(retrieved.len(), 10);
        for (i, result) in retrieved.iter().enumerate() {
            assert_eq!(*result, Some(values[i].clone()));
        }

        // Проверяем распределение по шардам
        let stats = store.sharding_info().1;
        assert_eq!(stats.total_keys, 10);
        assert!(stats.balance_ratio() <= 3.0); // Разумная балансировка

        Ok(())
    }

    #[test]
    fn test_aof_replay_and_cross_shard_rename() -> StoreResult<()> {
        // создаём отдельный temp file, чтобы можно было переоткрыть store и проверить replay
        let temp_file = NamedTempFile::new()?;
        let config = PersistentStoreConfig {
            sharding: ShardingConfig {
                num_shards: 4,
                enable_metrics: true,
                slow_operation_threshold_us: 1000,
            },
            sync_policy: SyncPolicy::Always,
            enable_operation_logging: false,
        };

        // 1) open, write, rename across shards
        let store = InPersistentStore::new(temp_file.path(), config.clone())?;

        // гарантируем разные шарды — пробуем искать пару ключей, которые попадают в разные шарды
        let k1 = Sds::from_str("k_0");
        let mut k2 = Sds::from_str("k_1");
        while store.index.shard_for_key(k1.as_bytes()) == store.index.shard_for_key(k2.as_bytes()) {
            // модифицируем k2, пока не попадут в разные шарды
            let n = rand::random::<u32>() % 10000;
            k2 = Sds::from_str(&format!("k_{}", n));
        }

        let v = Value::Str(Sds::from_str("value"));
        store.set(&k1, v.clone())?;

        // cross-shard rename
        store.rename(&k1, &k2)?;

        assert!(store.get(&k1)?.is_none());
        assert_eq!(store.get(&k2)?, Some(v.clone()));

        // Drop store (закроет aof), открыть новый и проверить replay
        drop(store);

        let reopened = InPersistentStore::new(temp_file.path(), config.clone())?;
        assert_eq!(reopened.get(&k1)?, None);
        assert_eq!(reopened.get(&k2)?, Some(v));

        Ok(())
    }

    #[test]
    fn test_flushdb_clears_all_and_metrics_zeroed() -> StoreResult<()> {
        let store = new_sharded_store(3)?;

        // наполняем базу
        for i in 0..30 {
            let k = Sds::from_str(&format!("flush_key_{}", i));
            let v = Value::Str(Sds::from_str(&format!("val_{}", i)));
            store.set(&k, v)?;
        }

        // убедимся, что ключи появились
        let stats_before = store.sharding_info().1;
        assert_eq!(stats_before.total_keys, 30);

        // flush
        store.flushdb()?;

        // все ключи должны быть отсутствовать
        for i in 0..30 {
            let k = Sds::from_str(&format!("flush_key_{}", i));
            assert!(store.get(&k)?.is_none());
        }

        let stats_after = store.sharding_info().1;
        assert_eq!(stats_after.total_keys, 0);

        Ok(())
    }

    #[test]
    fn test_geo_add_pos_and_dist() -> StoreResult<()> {
        let store = new_sharded_store(2)?;

        let key = Sds::from_str("geo_key");
        let m1 = Sds::from_str("m1");
        let m2 = Sds::from_str("m2");

        // добавляем две точки: (0,0) и (0,1) в метрах примерно 111km? (примерно)
        store.geo_add(&key, 0.0, 0.0, &m1)?;
        store.geo_add(&key, 0.0, 1.0, &m2)?;

        // позиции
        let p1 = store.geo_pos(&key, &m1)?;
        let p2 = store.geo_pos(&key, &m2)?;
        assert!(p1.is_some() && p2.is_some());

        // distance (в метрах) — должна быть положительная и не NaN
        let d = store.geo_dist(&key, &m1, &m2, "km")?;
        assert!(d.is_some());
        let d_km = d.unwrap();
        assert!(d_km > 0.0);

        Ok(())
    }
}
