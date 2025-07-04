use std::sync::Arc;

use super::Storage;
use crate::{GeoPoint, Sds, StoreError, StoreResult, Value};

/// Количество слотов в кластере.
pub const SLOT_COUNT: usize = 16384;

/// Реализация распределённого хранилища с маршрутизацией по слотам.
///
/// `InClusterStore` позволяет использовать несколько `Storage`-шардов.
/// Каждый ключ маршрутизируется в один из шардов на основе слота (slot),
/// вычисленного из значения ключа с использованием CRC16.
///
/// Распределение слотов между шардов происходит по кругу (round-robin)
/// при инициализации.
#[derive(Clone)]
pub struct InClusterStore {
    pub shards: Vec<Arc<dyn Storage>>,
    pub slots: Vec<usize>, // длина: 16384, каждый слот сопоставляется с индексом в `shards`
}

impl InClusterStore {
    /// Создаёт новый кластер, распределяя 16384 слота равномерно между шардов.
    ///
    /// # Паника
    /// Паника произойдёт, если `shards` пуст.
    pub fn new(shards: Vec<Arc<dyn Storage>>) -> Self {
        let mut slots = vec![0; SLOT_COUNT];
        for (i, slot) in slots.iter_mut().enumerate() {
            *slot = i % shards.len();
        }
        Self { shards, slots }
    }

    /// Вычисляет слот (от 0 до 16383) для данного ключа.
    ///
    /// Если в ключе присутствует подстрока в фигурных скобках (`{}`), хэшируется только она.
    /// Это позволяет контролировать маршрутизацию ключей на уровне приложения
    /// и гарантировать, что определённые ключи попадут на один и тот же шард.
    ///
    /// В противном случае, хэшируется весь ключ.
    ///
    /// Используется алгоритм CRC16 (XMODEM).
    fn key_slot(key: &Sds) -> usize {
        use crc16::{State, XMODEM};

        let bytes = key.as_bytes();

        if let Some(start) = bytes.iter().position(|&b| b == b'{') {
            if let Some(end) = bytes[start + 1..].iter().position(|&b| b == b'}') {
                let tag = &bytes[start + 1..start + 1 + end];
                let hash = State::<XMODEM>::calculate(tag);
                return (hash as usize) % SLOT_COUNT;
            }
        }

        let hash = State::<XMODEM>::calculate(bytes);
        (hash as usize) % SLOT_COUNT
    }

    /// Возвращает шард, на который должен быть направлен данный ключ.
    fn get_shard(
        &self,
        key: &Sds,
    ) -> Arc<dyn Storage> {
        let slot = Self::key_slot(key);
        let shard_idx = self.slots[slot];
        self.shards[shard_idx].clone()
    }
}

impl Storage for InClusterStore {
    /// Устанавливает значение для ключа на соответствующем шарде.
    fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()> {
        self.get_shard(key).set(key, value)
    }

    /// Получает значение ключа с соответствующего шарда.
    fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>> {
        self.get_shard(key).get(key)
    }

    /// Удаляет ключ с соответствующего шарда.
    fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool> {
        self.get_shard(key).del(key)
    }

    /// Устанавливает несколько пар ключ-значение.
    ///
    /// Все ключи могут располагаться на разных шардах.
    fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()> {
        for (k, v) in entries {
            self.set(k, v)?;
        }
        Ok(())
    }

    /// Получает значения для нескольких ключей.
    ///
    /// Все ключи могут располагаться на разных шардах.
    fn mget(
        &self,
        keys: &[&Sds],
    ) -> StoreResult<Vec<Option<Value>>> {
        keys.iter().map(|key| self.get(key)).collect()
    }

    /// Переименовывает ключ, если оба находятся на одном шарде.
    ///
    /// Возвращает ошибку `StoreError::WrongShard`, если `from` и `to` попадают на разные шарды.
    fn rename(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<()> {
        let from_shard = self.get_shard(from);
        let to_shard = self.get_shard(to);
        if !Arc::ptr_eq(&from_shard, &to_shard) {
            return Err(StoreError::WrongShard);
        }
        let val = self.get(from)?.ok_or(StoreError::KeyNotFound)?;
        self.del(from)?;
        self.set(to, val)?;
        Ok(())
    }

    /// То же, что и `rename`, но не переименовывает, если целевой ключ уже существует.
    ///
    /// Возвращает `Ok(true)`, если переименование произошло, иначе `Ok(false)`.
    fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool> {
        let from_shard = self.get_shard(from);
        let to_shard = self.get_shard(to);
        if !Arc::ptr_eq(&from_shard, &to_shard) {
            return Err(StoreError::WrongShard);
        }
        if self.get(to)?.is_some() {
            return Ok(false);
        }
        let val = self.get(from)?.ok_or(StoreError::KeyNotFound)?;
        self.del(from)?;
        self.set(to, val)?;
        Ok(true)
    }

    /// Полностью очищает все шардированные хранилища.
    fn flushdb(&self) -> StoreResult<()> {
        for shard in &self.shards {
            shard.flushdb()?;
        }
        Ok(())
    }

    /// Добавляет точку `(lon, lat)` с именем `member` в гео-набор по ключу `key`.
    ///
    /// Возвращает `Ok(true)`, если `member` был добавлен впервые,
    /// и `Ok(false)`, если он уже присутствовал в наборе.
    fn geo_add(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        member: &Sds,
    ) -> StoreResult<bool> {
        self.get_shard(key).geo_add(key, lon, lat, member)
    }

    /// Вычисляет расстояние между двумя членами `member1` и `member2` в указанной единице `unit`.
    ///
    /// Поддерживаемые единицы: `"m"` (метры), `"km"`, `"mi"`, `"ft"`.
    /// Если один из членов не найден или ключ отсутствует, возвращает `Ok(None)`.
    fn geo_dist(
        &self,
        key: &Sds,
        m1: &Sds,
        m2: &Sds,
        unit: &str,
    ) -> StoreResult<Option<f64>> {
        self.get_shard(key).geo_dist(key, m1, m2, unit)
    }

    /// Возвращает координаты `[lon, lat]` для каждого запрошенного `member`.
    ///
    /// Если член не найден, в возвращаемом массиве на его месте будет `Value::Null`.
    fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>> {
        self.get_shard(key).geo_pos(key, member)
    }

    /// Ищет всех членов в радиусе `radius` от точки `(lon, lat)`.
    ///
    /// `radius` задаётся в единицах `unit` (`"m"`, `"km"`, `"mi"`, `"ft"`).
    /// Возвращает вектор кортежей `(member, distance, GeoPoint)`.
    fn geo_radius(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        self.get_shard(key).geo_radius(key, lon, lat, radius, unit)
    }

    /// То же, что `geo_radius`, но центр радиуса определяется по координатам `member`.
    ///
    /// Если `member` не найден, возвращает пустой вектор.
    fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        self.get_shard(key)
            .geo_radius_by_member(key, member, radius, unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    /// Создаёт кластер `InClusterStore` с двумя in-memory хранилищами.
    ///
    /// Используется в тестах для проверки маршрутизации ключей между шардами.
    fn make_cluster() -> InClusterStore {
        #[allow(clippy::arc_with_non_send_sync)]
        let s1 = Arc::new(InMemoryStore::new());
        #[allow(clippy::arc_with_non_send_sync)]
        let s2 = Arc::new(InMemoryStore::new());
        InClusterStore::new(vec![s1, s2])
    }

    /// Тест проверяет, что слот, получаемый из ключа, всегда в пределах
    /// диапазона [0, SLOT_COUNT).
    #[test]
    fn test_key_slot_range() {
        let key = Sds::from_str("kin");
        let slot = InClusterStore::key_slot(&key);
        assert!(slot < SLOT_COUNT);
    }

    /// Тест проверяет, что ключи направляются к правильным шардам при `set` и `get`.
    #[test]
    fn test_set_get_routes_to_correct_shard() {
        let cluster = make_cluster();
        let k1 = Sds::from_str("alpha");
        let v1 = Value::Str(Sds::from_str("A"));

        let k2 = Sds::from_str("beta");
        let v2 = Value::Str(Sds::from_str("B"));

        cluster.set(&k1, v1.clone()).unwrap();
        cluster.set(&k2, v2.clone()).unwrap();

        assert_eq!(cluster.get(&k1).unwrap(), Some(v1));
        assert_eq!(cluster.get(&k2).unwrap(), Some(v2));
    }

    /// Тест проверяет, что `rename` срабатывает, если ключи находятся на одном шарде.
    #[test]
    fn test_rename_same_shard_succeeds() {
        let cluster = make_cluster();
        let from = Sds::from_str("{same}");
        let to = Sds::from_str("{same}new");

        assert_eq!(
            InClusterStore::key_slot(&from),
            InClusterStore::key_slot(&to)
        );

        cluster.set(&from, Value::Int(42)).unwrap();
        cluster.rename(&from, &to).unwrap();

        assert_eq!(cluster.get(&from).unwrap(), None);
        assert_eq!(cluster.get(&to).unwrap(), Some(Value::Int(42)));
    }

    /// Тест проверяет, что попытка `rename` между разными шардами вызывает ошибку `WrongShard`.
    #[test]
    fn test_rename_different_shards_errors() {
        let mut cluster = make_cluster();

        let a = Sds::from_str("a");
        let b = Sds::from_str("b");

        let slot_a = InClusterStore::key_slot(&a);
        let slot_b = InClusterStore::key_slot(&b);
        cluster.slots[slot_a] = 0;
        cluster.slots[slot_b] = 1;

        cluster.set(&a, Value::Int(7)).unwrap();
        let err = cluster.rename(&a, &b).unwrap_err();
        assert!(matches!(err, StoreError::WrongShard));
    }

    /// Тест проверяет, что `flushdb` очищает все шарды.
    #[test]
    fn test_flushdb_clears_all_shards() {
        let cluster = make_cluster();
        cluster.set(&Sds::from_str("one"), Value::Int(1)).unwrap();
        cluster.set(&Sds::from_str("two"), Value::Int(2)).unwrap();

        assert!(cluster.get(&Sds::from_str("one")).unwrap().is_some());
        assert!(cluster.get(&Sds::from_str("two")).unwrap().is_some());

        cluster.flushdb().unwrap();

        assert_eq!(cluster.get(&Sds::from_str("one")).unwrap(), None);
        assert_eq!(cluster.get(&Sds::from_str("two")).unwrap(), None);
    }

    /// Тест проверяет, что ключи с одинаковым хеш-тегом `{tag}` направляются в
    /// один и тот же слот, даже если остальные части ключа отличаются.
    #[test]
    fn test_key_slot_tag_ignores_outside() {
        let a = Sds::from_str("{tag}");
        let b = Sds::from_str("{tag}kin");
        let c = Sds::from_str("x{tag}kin");

        let sa = InClusterStore::key_slot(&a);
        let sb = InClusterStore::key_slot(&b);
        let sc = InClusterStore::key_slot(&c);
        assert_eq!(sa, sb);
        assert_eq!(sb, sc);
    }

    /// Тест для geo_add и geo_pos: проверяем добавление точки и её получение.
    #[test]
    fn test_cluster_geo_add_and_pos() {
        let cluster = make_cluster();
        let key = Sds::from_str("cities");
        let paris = Sds::from_str("paris");
        let berlin = Sds::from_str("berlin");

        // Добавляем в кластер
        assert!(cluster.geo_add(&key, 2.3522, 48.8566, &paris).unwrap());
        assert!(cluster.geo_add(&key, 13.4050, 52.5200, &berlin).unwrap());

        // Повторное добавление того же члена должно вернуть false
        assert!(!cluster.geo_add(&key, 2.3522, 48.8566, &paris).unwrap());

        // geo_pos
        let p = cluster.geo_pos(&key, &paris).unwrap().unwrap();
        assert!((p.lon - 2.3522).abs() < 1e-6);
        assert!((p.lat - 48.8566).abs() < 1e-6);
    }

    /// Тест для geo_dist: проверяем вычисление расстояния между двумя точками.
    #[test]
    fn test_cluster_geo_dist() {
        let cluster = make_cluster();
        let key = Sds::from_str("cities");
        let paris = Sds::from_str("paris");
        let berlin = Sds::from_str("berlin");

        cluster.geo_add(&key, 2.3522, 48.8566, &paris).unwrap();
        cluster.geo_add(&key, 13.4050, 52.5200, &berlin).unwrap();

        let d_km = cluster
            .geo_dist(&key, &paris, &berlin, "km")
            .unwrap()
            .unwrap();
        assert!((d_km - 878.0).abs() < 10.0);

        let d_m = cluster
            .geo_dist(&key, &paris, &berlin, "m")
            .unwrap()
            .unwrap();
        assert!((d_m - 878_000.0).abs() < 20_000.0);
    }

    /// Тест для geo_radius: проверяем поиск точек в радиусе.
    #[test]
    fn test_cluster_geo_radius() {
        let cluster = make_cluster();
        let key = Sds::from_str("landmarks");
        let center = Sds::from_str("center");
        let near = Sds::from_str("near");
        let far = Sds::from_str("far");

        cluster.geo_add(&key, 0.0, 0.0, &center).unwrap();
        cluster.geo_add(&key, 0.001, 0.001, &near).unwrap();
        cluster.geo_add(&key, 10.0, 10.0, &far).unwrap();

        // radius = 0.2 km
        let results = cluster.geo_radius(&key, 0.0, 0.0, 0.2, "km").unwrap();
        let members: Vec<_> = results.iter().map(|(m, _, _)| m.clone()).collect();

        assert!(members.contains(&"center".to_string()));
        assert!(members.contains(&"near".to_string()));
        assert!(!members.contains(&"far".to_string()));
    }

    /// Тест для geo_radius_by_member: поиск по существующему члену.
    #[test]
    fn test_cluster_geo_radius_by_member() {
        let cluster = make_cluster();
        let key = Sds::from_str("points");
        let origin = Sds::from_str("origin");
        let east = Sds::from_str("east");
        let north = Sds::from_str("north");
        let faraway = Sds::from_str("faraway");

        cluster.geo_add(&key, 0.0, 0.0, &origin).unwrap();
        cluster.geo_add(&key, 0.002, 0.0, &east).unwrap();
        cluster.geo_add(&key, 0.0, 0.002, &north).unwrap();
        cluster.geo_add(&key, 1.0, 1.0, &faraway).unwrap();

        let results = cluster
            .geo_radius_by_member(&key, &origin, 0.3, "km")
            .unwrap();
        let members: Vec<_> = results.iter().map(|(m, _, _)| m.clone()).collect();

        assert!(members.contains(&"origin".to_string()));
        assert!(members.contains(&"east".to_string()));
        assert!(members.contains(&"north".to_string()));
        assert!(!members.contains(&"faraway".to_string()));
    }
}
