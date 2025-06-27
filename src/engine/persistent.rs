use std::{collections::HashMap, io::Cursor, path::Path, sync::Mutex};

use super::{
    aof::{AofOp, SyncPolicy},
    write_stream, AofLog, Storage, StreamReader,
};
use crate::{GeoPoint, GeoSet, Sds, StoreError, StoreResult, Value};

/// Хранилище с поддержкой постоянства через AOF (Append-Only File).
/// Ключи и значения находятся в памяти, но изменения логируются на диск.
pub struct InPersistentStore {
    /// Основной in-memory индекс.
    index: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    /// Журнал AOF, логирующий изменения.
    aof: Mutex<AofLog>,
}

impl InPersistentStore {
    /// Создаёт новое хранилище с журналом AOF.
    /// При инициализации восстанавливает состояние из AOF.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StoreError> {
        let mut aof = AofLog::open(path, SyncPolicy::Always)?;
        let mut index = HashMap::new();

        // Восстановление состояния из журнала
        aof.replay(|op, key, val| match op {
            AofOp::Set => {
                if let Some(value) = val {
                    index.insert(key, value);
                }
            }
            AofOp::Del => {
                index.remove(&key);
            }
        })?;

        Ok(Self {
            index: Mutex::new(index),
            aof: Mutex::new(aof),
        })
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
        self.aof.lock().unwrap().append_set(key_b, &val_b)?;
        self.index.lock().unwrap().insert(key_b.to_vec(), val_b);
        Ok(())
    }

    /// Получает значение по ключу, если оно существует.
    fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>> {
        let key_b = key.as_bytes();
        let map = self.index.lock().unwrap();
        match map.get(key_b) {
            Some(val) => Ok(Some(Value::from_bytes(val)?)),
            None => Ok(None),
        }
    }

    /// Удаляет ключ, если он есть, и логирует удаление в AOF.
    fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool> {
        let key_b = key.as_bytes();
        let mut map = self.index.lock().unwrap();
        if map.remove(key_b).is_some() {
            self.aof.lock().unwrap().append_del(key_b)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Устанавливает несколько пар ключ-значение сразу.
    /// Каждая операция логируется в AOF.
    fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()> {
        let mut log = self.aof.lock().unwrap();
        let mut map = self.index.lock().unwrap();
        for (key, val) in entries {
            let key_b = key.as_bytes();
            let val_b = val.to_bytes();
            log.append_set(key_b, &val_b)?;
            map.insert(key_b.to_vec(), val_b);
        }
        Ok(())
    }

    /// Получает значения по списку ключей.
    fn mget(
        &self,
        keys: &[&Sds],
    ) -> StoreResult<Vec<Option<Value>>> {
        let map = self.index.lock().unwrap();
        let mut result = Vec::with_capacity(keys.len());
        for key in keys {
            let key_b = key.as_bytes();
            if let Some(val) = map.get(key_b) {
                result.push(Some(Value::from_bytes(val)?));
            } else {
                result.push(None);
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
        let mut map = self.index.lock().unwrap();
        let from_b = from.as_bytes();
        let to_b = to.as_bytes();

        if let Some(val) = map.remove(from_b) {
            self.aof.lock().unwrap().append_del(from_b)?;
            self.aof.lock().unwrap().append_set(to_b, &val)?;
            map.insert(to_b.to_vec(), val);
            Ok(())
        } else {
            Err(StoreError::KeyNotFound)
        }
    }

    /// Как `rename`, но не переименовывает, если целевой ключ уже существует.
    fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool> {
        let mut map = self.index.lock().unwrap();
        let from_b = from.as_bytes();
        let to_b = to.as_bytes();

        // 1. Если исходного ключа нет — ошибка
        if !map.contains_key(from_b) {
            return Err(StoreError::KeyNotFound);
        }
        // 2. Если целевой уже есть — ничего не делаем
        if map.contains_key(to_b) {
            return Ok(false);
        }
        // 3. Перемещаем ключ, логируем обе операции
        if let Some(val) = map.remove(from_b) {
            let mut aof = self.aof.lock().unwrap();
            aof.append_del(from_b)?;
            aof.append_set(to_b, &val)?;
            map.insert(to_b.to_vec(), val);
            return Ok(true);
        }
        // По идее unreachable, но на всякий
        Ok(false)
    }

    /// Очищает всё in-memory содержимое.
    /// AOF не трогаем (на практике можно реализовать truncate).
    fn flushdb(&self) -> StoreResult<()> {
        let mut map = self.index.lock().unwrap();
        map.clear();
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
        let mut map = self.index.lock().unwrap();

        // 1) собрать существующий GeoSet из streaming-данных
        let mut gs = if let Some(raw) = map.get(key_b) {
            let mut rdr = StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
            let mut tmp = GeoSet::new();
            while let Some(Ok((_, val))) = rdr.next() {
                if let Value::Array(arr) = val {
                    if let [Value::Str(m), Value::Float(dlon), Value::Float(dlat)] = &arr[..] {
                        tmp.add(m.to_string(), *dlon, *dlat);
                    }
                }
            }
            tmp
        } else {
            GeoSet::new()
        };

        // 2) собственно add
        let existed = gs.get(member.as_str()?).is_some();
        gs.add(member.to_string(), lon, lat);
        let added = !existed;

        // 3) сериализовать обратно в streaming-формате
        let mut buf = Vec::new();
        let entries = gs.iter().map(|(m, point)| {
            let v = Value::Array(vec![
                Value::Str(Sds::from_str(m)),
                Value::Float(point.lon),
                Value::Float(point.lat),
            ]);
            (Sds::from_str(m), v)
        });
        write_stream(&mut buf, entries).map_err(StoreError::Io)?;

        // 4) лог и сохранение
        {
            let mut aof = self.aof.lock().unwrap();
            aof.append_set(key_b, &buf).map_err(StoreError::Io)?;
        }
        map.insert(key_b.to_vec(), buf);
        Ok(added)
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
        let map = self.index.lock().unwrap();
        let raw = match map.get(key_b) {
            Some(r) => r,
            None => return Ok(None),
        };

        // Восстанавливаем GeoSet
        let mut gs = GeoSet::new();
        let mut rdr = StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
        while let Some(Ok((_, val))) = rdr.next() {
            if let Value::Array(arr) = val {
                if let [Value::Str(m), Value::Float(lon0), Value::Float(lat0)] = &arr[..] {
                    gs.add(m.to_string(), *lon0, *lat0);
                }
            }
        }

        // Конвертируем Sds → &str
        let m1 = member1.as_str()?;
        let m2 = member2.as_str()?;

        // Считаем и переводим в нужные единицы
        let meters = gs.dist(m1, m2);
        let res = meters.map(|m| match unit {
            "km" => m / 1000.0,
            "mi" => m / 1609.344,
            "ft" => m * 3.28084,
            _ => m,
        });
        Ok(res)
    }

    /// Возвращает координаты `member` в GeoPoint, или `None`, если member не найден.
    fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>> {
        let key_b = key.as_bytes();
        let map = self.index.lock().unwrap();
        let raw = match map.get(key_b) {
            Some(r) => r,
            None => return Ok(None),
        };

        let mut rdr = StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
        while let Some(Ok((k, val))) = rdr.next() {
            if k.as_str()? == member.as_str()? {
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
        let map = self.index.lock().unwrap();
        let raw = match map.get(key_b) {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        // восстановить GeoSet
        let mut gs = GeoSet::new();
        let mut rdr = StreamReader::new(Cursor::new(raw.as_slice())).map_err(StoreError::Io)?;
        while let Some(Ok((_, val))) = rdr.next() {
            if let Value::Array(arr) = val {
                if let [Value::Str(m), Value::Float(lon0), Value::Float(lat0)] = &arr[..] {
                    gs.add(m.to_string(), *lon0, *lat0);
                }
            }
        }

        // radius → метры
        let r_m = match unit {
            "km" => radius * 1000.0,
            "mi" => radius * 1609.344,
            "ft" => radius / 3.28084,
            _ => radius,
        };

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
    }

    /// Аналогично `geo_radius`, но центр задаётся существующим `member`.
    fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        // сначала позиция
        let center = self.geo_pos(key, member)?;
        if let Some(GeoPoint { lon, lat }) = center {
            self.geo_radius(key, lon, lat, radius, unit)
        } else {
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::*;

    // Вспомогательный fn для создания нового в постоянном хранилище с временным файлом AOF.
    fn new_store() -> Result<InPersistentStore, StoreError> {
        let temp_file = NamedTempFile::new()?;
        InPersistentStore::new(temp_file.path())
    }

    /// This test checks if the `set` and `get` methods works correctly.
    #[test]
    fn test_set_and_get() -> StoreResult<()> {
        let store = new_store()?;

        let key = Sds::from_str("key1");
        let value = Value::Str(Sds::from_str("value1"));

        store.set(&key, value.clone())?;

        let retrieved = store.get(&key)?;
        assert_eq!(retrieved, Some(value));
        Ok(())
    }

    /// This test checks if the `del` method works correctly.
    #[test]
    fn test_del() -> StoreResult<()> {
        let store = new_store()?;

        let key = Sds::from_str("key1");
        let value = Value::Str(Sds::from_str("value1"));

        store.set(&key, value.clone())?;

        let del_count = store.del(&key)?;
        assert!(del_count);

        let retrieved = store.get(&key)?;
        assert_eq!(retrieved, None);

        Ok(())
    }

    /// This test checks if the `mset` and `mget` methods work correctly.
    #[test]
    fn test_mset_and_mget() -> StoreResult<()> {
        let store = new_store()?;

        let k1 = Sds::from_str("key1");
        let k2 = Sds::from_str("key2");

        let entries = vec![
            (&k1, Value::Str(Sds::from_str("value1"))),
            (&k2, Value::Str(Sds::from_str("value2"))),
        ];

        store.mset(entries)?;

        let keys = vec![&k1, &k2];
        let retrieved = store.mget(&keys)?;

        assert_eq!(
            retrieved,
            vec![
                Some(Value::Str(Sds::from_str("value1"))),
                Some(Value::Str(Sds::from_str("value2"))),
            ]
        );

        Ok(())
    }

    /// This test checks if the `rename` method works correctly.
    #[test]
    fn test_rename() -> StoreResult<()> {
        let store = new_store()?;

        let key1 = Sds::from_str("key1");
        let key2 = Sds::from_str("key2");
        let value = Value::Str(Sds::from_str("value1"));

        store.set(&key1, value)?;

        store.rename(&key1, &key2)?;

        let retrieved_old = store.get(&key1)?;
        assert_eq!(retrieved_old, None);

        let retrieved_new = store.get(&key2)?;
        assert_eq!(retrieved_new, Some(Value::Str(Sds::from_str("value1"))));

        Ok(())
    }

    /// This test checks if the `renamenx` method works correctly.
    #[test]
    fn test_renamenx() -> StoreResult<()> {
        let store = new_store()?;

        let key1 = Sds::from_str("key1");
        let key2 = Sds::from_str("key2");
        let val = Value::Str(Sds::from_str("value1"));

        store.set(&key1, val.clone())?;
        assert_eq!(store.get(&key2)?, None);

        assert!(store.renamenx(&key1, &key2)?);
        assert_eq!(store.get(&key1)?, None);
        assert_eq!(store.get(&key2)?, Some(val.clone()));

        store.set(&key1, Value::Str(Sds::from_str("other")))?;
        assert!(!store.renamenx(&key1, &key2)?);

        Ok(())
    }

    /// This test checks if the `flushdb` method works correctly.
    #[test]
    fn test_flushdb() -> StoreResult<()> {
        let store = new_store()?;

        let key1 = Sds::from_str("key1");
        let key2 = Sds::from_str("key2");
        let value = Value::Str(Sds::from_str("value1"));

        store.set(&key1, value.clone())?;
        store.set(&key2, value)?;

        store.flushdb()?;

        let retrieved1 = store.get(&key1)?;
        let retrieved2 = store.get(&key2)?;

        assert_eq!(retrieved1, None);
        assert_eq!(retrieved2, None);

        Ok(())
    }
}
