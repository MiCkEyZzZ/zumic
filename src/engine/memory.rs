use std::sync::Arc;

use dashmap::DashMap;

use crate::{GeoPoint, GeoSet, Sds, Storage, StoreError, StoreResult, Value};

/// Потокобезопасное in-memory хранилище ключ-значение.
///
/// Использует [`DashMap`] с обёрткой [`Arc`] для безопасного совместного использования.
///
/// Содержит реализацию интерфейса [`Storage`] и поддерживает базовые операции,
/// включая множественные вставки/чтения (`mset` / `mget`) и атомарные переименования (`rename`, `renamenx`).
#[derive(Debug)]
pub struct InMemoryStore {
    #[allow(clippy::arc_with_non_send_sync)]
    pub data: Arc<DashMap<Sds, Value>>,
    // GEO-ключи → GeoSet
    #[allow(clippy::arc_with_non_send_sync)]
    geo: Arc<DashMap<Sds, GeoSet>>,
}

impl InMemoryStore {
    /// Создаёт новый, пустой `InMemoryStore`.
    pub fn new() -> Self {
        Self {
            #[allow(clippy::arc_with_non_send_sync)]
            data: Arc::new(DashMap::new()),
            #[allow(clippy::arc_with_non_send_sync)]
            geo: Arc::new(DashMap::new()),
        }
    }

    /// Возвращает итератор по всем ключам и значениям в хранилище.
    ///
    /// Каждый элемент возвращается в виде клонированной пары `(Sds, Value)`.
    pub fn iter(&self) -> impl Iterator<Item = (Sds, Value)> + '_ {
        self.data
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
    }
}

impl Storage for InMemoryStore {
    /// Устанавливает значение для ключа.
    ///
    /// Перезаписывает существующее значение, если ключ уже существует.
    fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()> {
        self.data.insert(key.clone(), value);
        Ok(())
    }

    /// Получает значение по ключу.
    ///
    /// Возвращает `Ok(Some(value))`, если ключ существует, иначе `Ok(None)`.
    fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>> {
        Ok(self.data.get(key).map(|entry| entry.value().clone()))
    }

    /// Удаляет ключ. Возвращает `Ok(true)`, если ключ действительно был удалён,
    /// и `Ok(false)`, если ключ не существовал.
    fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool> {
        Ok(self.data.remove(key).is_some())
    }

    /// Массовая установка значений.
    ///
    /// Устанавливает все переданные пары ключ-значение. Ключи клонируются.
    fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()> {
        for (key, value) in entries {
            self.data.insert(key.clone(), value);
        }
        Ok(())
    }

    /// Массовое получение значений по ключам.
    ///
    /// Возвращает вектор опциональных значений, соответствующих переданным ключам.
    fn mget(
        &self,
        keys: &[&Sds],
    ) -> StoreResult<Vec<Option<Value>>> {
        let result = keys
            .iter()
            .map(|key| self.data.get(key).map(|entry| entry.value().clone()))
            .collect();
        Ok(result)
    }

    /// Переименовывает ключ.
    ///
    /// Удаляет старый ключ и вставляет значение по новому.
    /// Возвращает ошибку `KeyNotFound`, если исходный ключ не существует.
    fn rename(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<()> {
        if let Some((_, value)) = self.data.remove(from) {
            self.data.insert(to.clone(), value);
            Ok(())
        } else {
            Err(StoreError::KeyNotFound)
        }
    }

    /// Переименовывает ключ, если целевой ещё не существует.
    ///
    /// Возвращает `Ok(true)`, если переименование прошло успешно.
    /// Возвращает `Ok(false)`, если целевой ключ уже существует.
    /// Возвращает `Err(KeyNotFound)`, если исходный ключ отсутствует.
    fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool> {
        if self.data.contains_key(to) {
            return Ok(false);
        }
        if let Some((_, value)) = self.data.remove(from) {
            self.data.insert(to.clone(), value);
            Ok(true)
        } else {
            Err(StoreError::KeyNotFound)
        }
    }

    /// Очищает всё содержимое хранилища.
    fn flushdb(&self) -> StoreResult<()> {
        self.data.clear();
        Ok(())
    }

    fn geo_add(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        member: &Sds,
    ) -> StoreResult<bool> {
        let mut entry = self.geo.entry(key.clone()).or_default();
        let member_str = member.as_str()?;
        let existed = entry.get(member_str).is_some();
        entry.add(member.to_string(), lon, lat);
        Ok(!existed)
    }

    fn geo_dist(
        &self,
        key: &Sds,
        member1: &Sds,
        member2: &Sds,
        unit: &str,
    ) -> StoreResult<Option<f64>> {
        let set = match self.geo.get(key) {
            Some(s) => s,
            None => return Ok(None),
        };
        let meters = set.dist(&member1.to_string(), &member2.to_string());
        let converted = meters.map(|d| match unit {
            "km" => d / 1000.0,
            "mi" => d / 1609.344,
            "ft" => d / 3.28084,
            _ => d,
        });
        Ok(converted)
    }

    fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>> {
        let set = match self.geo.get(key) {
            Some(s) => s,
            None => return Ok(None),
        };
        let member_str = member.as_str()?;
        Ok(set.get(member_str))
    }

    fn geo_radius(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        let set = match self.geo.get(key) {
            Some(s) => s,
            None => return Ok(vec![]),
        };
        // 1. convert to meters
        let radius_m = match unit {
            "km" => radius * 1000.0,
            "mi" => radius * 1609.344,
            "ft" => radius / 3.28084,
            _ => radius, // assume meters
        };
        // filter in meters
        let mut raw = set.radius(lon, lat, radius_m);
        // 2. convert back for output
        let mut out = Vec::with_capacity(raw.len());
        for (member, dist_m) in raw.drain(..) {
            let dist_out = match unit {
                "km" => dist_m / 1000.0,
                "mi" => dist_m / 1609.344,
                "ft" => dist_m * 3.28084,
                _ => dist_m,
            };
            let point = set.get(&member).unwrap();
            out.push((member, dist_out, point));
        }
        Ok(out)
    }

    fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        let set = match self.geo.get(key) {
            Some(s) => s,
            None => return Ok(vec![]),
        };
        let m_str = member.as_str()?;
        let center = match set.get(m_str) {
            Some(p) => p,
            None => return Ok(vec![]),
        };
        self.geo_radius(key, center.lon, center.lat, radius, unit)
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(data: &str) -> Sds {
        Sds::from(data.as_bytes())
    }

    /// Основной тест на установку и получение значения.
    /// Проверяет, что значение можно корректно записать и получить из хранилища.
    #[test]
    fn test_set_and_get() {
        let store = InMemoryStore::new();
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));

        store.set(&k, v.clone()).unwrap();
        let got = store.get(&k).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Проверяет, что повторная установка значения по тому же ключу
    /// перезаписывает старое значение.
    /// Удостоверяется, что вызов `set` на уже существующем ключе обновляет значение.
    #[test]
    fn test_overwrite_value() {
        let store = InMemoryStore::new();
        let k = key("overwrite");

        store.set(&k, Value::Str(Sds::from_str("one"))).unwrap();
        store.set(&k, Value::Str(Sds::from_str("two"))).unwrap();

        let got = store.get(&k).unwrap();
        assert_eq!(got, Some(Value::Str(Sds::from_str("two"))));
    }

    /// Проверяет, что ключ можно удалить, и после этого он становится недоступным.
    /// Удостоверяется, что после вызова `del` ключ больше не извлекается.
    #[test]
    fn test_delete() {
        let store = InMemoryStore::new();
        let k = key("key_to_delete");
        let v = Value::Str(Sds::from_str("some_value"));

        store.set(&k, v).unwrap();
        store.del(&k).unwrap();

        let got = store.get(&k).unwrap();
        assert_eq!(got, None);
    }

    /// Проверяет, что попытка получить значение по несуществующему ключу возвращает None.
    /// Удостоверяется, что получение отсутствующего ключа возвращает `None`.
    #[test]
    fn test_get_nonexistent_key() {
        let store = InMemoryStore::new();
        let got = store.get(&key("missing")).unwrap();
        assert_eq!(got, None);
    }

    /// Проверяет, что удаление несуществующего ключа не вызывает ошибку.
    /// Удостоверяется, что вызов `del` на несуществующем ключе проходит без ошибки.
    #[test]
    fn test_delete_nonexistent_key() {
        let store = InMemoryStore::new();
        // Deleting a non-existent key should not cause an error.
        assert!(store.del(&key("nope")).is_ok());
    }

    /// Тестирует массовую установку и получение значений.
    /// Проверяет, что можно установить и получить несколько пар ключ-значение одновременно,
    /// и что отсутствующие ключи возвращают `None`.
    #[test]
    fn test_mset_and_mget() {
        let store = InMemoryStore::new();

        let k1 = key("key1");
        let k2 = key("key2");
        let k3 = key("key3");
        let kmissing = key("missing");

        let entries = vec![
            (&k1, Value::Int(1)),
            (&k2, Value::Int(2)),
            (&k3, Value::Int(3)),
        ];
        store.mset(entries).unwrap();

        let keys: Vec<&Sds> = vec![&k1, &k2, &k3, &kmissing];
        let result = store.mget(&keys).unwrap();

        assert_eq!(
            result,
            vec![
                Some(Value::Int(1)),
                Some(Value::Int(2)),
                Some(Value::Int(3)),
                None
            ]
        );
    }

    /// Проверяет, что переименование существующего ключа работает корректно.
    /// Удостоверяется, что ключ переносится на новое имя, а старый ключ становится недоступным.
    #[test]
    fn test_rename() {
        let store = InMemoryStore::new();
        store.set(&key("old"), Value::Int(123)).unwrap();

        store.rename(&key("old"), &key("new")).unwrap();
        assert!(store.get(&key("old")).unwrap().is_none());
        assert_eq!(store.get(&key("new")).unwrap(), Some(Value::Int(123)));
    }

    /// Проверяет, что попытка переименовать несуществующий ключ вызывает ошибку
    /// с кодом KeyNotFound.
    /// Удостоверяется, что переименование отсутствующего ключа возвращает ошибку.
    #[test]
    fn test_rename_nonexistent_key() {
        let store = InMemoryStore::new();
        let result = store.rename(&key("does_not_exist"), &key("whatever"));
        assert!(matches!(result, Err(StoreError::KeyNotFound)));
    }

    /// Тестирует метод renamenx: переименование происходит только
    /// если целевой ключ не существует.
    /// Удостоверяется, что переименование работает, только если целевой ключ отсутствует.
    #[test]
    fn test_renamenx_success() {
        let store = InMemoryStore::new();
        store
            .set(&key("old"), Value::Str(Sds::from_str("val")))
            .unwrap();

        let ok = store.renamenx(&key("old"), &key("new")).unwrap();
        assert!(ok);
        assert!(store.get(&key("old")).unwrap().is_none());
        assert_eq!(
            store.get(&key("new")).unwrap(),
            Some(Value::Str(Sds::from_str("val")))
        );
    }

    /// Проверяет, что renamenx не выполняется, если целевой ключ уже существует.
    /// Удостоверяется, что переименование не происходит, если целевой ключ занят.
    #[test]
    fn test_renamenx_existing_target() {
        let store = InMemoryStore::new();
        store.set(&key("old"), Value::Int(1)).unwrap();
        store.set(&key("new"), Value::Int(2)).unwrap();

        let ok = store.renamenx(&key("old"), &key("new")).unwrap();
        assert!(!ok); // Expects false since the target key already exists.
        assert_eq!(store.get(&key("old")).unwrap(), Some(Value::Int(1)));
        assert_eq!(store.get(&key("new")).unwrap(), Some(Value::Int(2)));
    }

    /// Проверяет, что метод flushdb удаляет все ключи и значения из хранилища.
    /// Удостоверяется, что вызов `flushdb` полностью очищает хранилище.
    #[test]
    fn test_flushdb() {
        let store = InMemoryStore::new();
        store.set(&key("one"), Value::Int(1)).unwrap();
        store.set(&key("two"), Value::Int(2)).unwrap();

        store.flushdb().unwrap();

        assert!(store.get(&key("one")).unwrap().is_none());
        assert!(store.get(&key("two")).unwrap().is_none());
    }

    /// Проверяет корректную обработку пустого ключа.
    /// Удостоверяется, что пустой ключ можно сохранить и получить из хранилища.
    #[test]
    fn test_empty_key() {
        let store = InMemoryStore::new();
        let empty = key("");
        store.set(&empty, Value::Int(42)).unwrap();
        assert_eq!(store.get(&empty).unwrap(), Some(Value::Int(42)));
    }

    /// Проверяет обработку очень длинных ключей и значений.
    /// Удостоверяется, что хранилище может работать с ключами и значениями произвольной длины.
    #[test]
    fn test_very_long_key_and_value() {
        let store = InMemoryStore::new();
        let long_key = key(&"k".repeat(10_000));
        let long_value = Value::Str(Sds::from("v".repeat(100_000).as_bytes()));

        store.set(&long_key, long_value.clone()).unwrap();
        assert_eq!(store.get(&long_key).unwrap(), Some(long_value));
    }

    #[test]
    fn test_geo_add_and_pos() {
        let store = InMemoryStore::new();
        let cities_key = key("cities");
        let paris_member = key("paris");

        let added = store
            .geo_add(&cities_key, 2.3522, 48.8566, &paris_member)
            .unwrap();
        assert!(added);

        let added_again = store
            .geo_add(&cities_key, 2.3522, 48.8566, &paris_member)
            .unwrap();
        assert!(!added_again);

        let pos = store.geo_pos(&cities_key, &paris_member).unwrap();
        assert!(pos.is_some());
        let point = pos.unwrap();
        assert!((point.lon - 2.3522).abs() < 1e-6);
        assert!((point.lat - 48.8566).abs() < 1e-6);
    }

    #[test]
    fn test_geo_dist() {
        let store = InMemoryStore::new();
        let cities_key = key("cities");
        let paris = key("paris");
        let berlin = key("berlin");

        store.geo_add(&cities_key, 2.3522, 48.8566, &paris).unwrap();
        store
            .geo_add(&cities_key, 13.4050, 52.5200, &berlin)
            .unwrap();

        let dist_km = store
            .geo_dist(&cities_key, &paris, &berlin, "km")
            .unwrap()
            .unwrap();
        assert!((dist_km - 878.0).abs() < 10.0);

        let dist_m = store
            .geo_dist(&cities_key, &paris, &berlin, "m")
            .unwrap()
            .unwrap();
        assert!((dist_m - 878_000.0).abs() < 20_000.0);
    }

    #[test]
    fn test_geo_radius() {
        let store = InMemoryStore::new();
        let landmarks_key = key("landmarks");

        store
            .geo_add(&landmarks_key, 0.0, 0.0, &key("center"))
            .unwrap();
        store
            .geo_add(&landmarks_key, 0.001, 0.001, &key("near"))
            .unwrap();
        store
            .geo_add(&landmarks_key, 10.0, 10.0, &key("far"))
            .unwrap();

        // Радиус 200 м (0.2 км)
        let results = store
            .geo_radius(&landmarks_key, 0.0, 0.0, 0.2, "km")
            .unwrap();
        let members: Vec<_> = results.iter().map(|(m, _, _)| m.clone()).collect();

        assert!(members.contains(&"center".to_string()));
        assert!(members.contains(&"near".to_string()));
        assert!(!members.contains(&"far".to_string()));
    }

    #[test]
    fn test_geo_radius_by_member() {
        let store = InMemoryStore::new();
        let points_key = key("points");

        store
            .geo_add(&points_key, 0.0, 0.0, &key("origin"))
            .unwrap();
        store
            .geo_add(&points_key, 0.002, 0.0, &key("east"))
            .unwrap();
        store
            .geo_add(&points_key, 0.0, 0.002, &key("north"))
            .unwrap();
        store
            .geo_add(&points_key, 1.0, 1.0, &key("faraway"))
            .unwrap();

        let results = store
            .geo_radius_by_member(&points_key, &key("origin"), 0.3, "km")
            .unwrap();
        let members: Vec<_> = results.iter().map(|(m, _, _)| m.clone()).collect();

        assert!(members.contains(&"origin".to_string()));
        assert!(members.contains(&"east".to_string()));
        assert!(members.contains(&"north".to_string()));
        assert!(!members.contains(&"faraway".to_string()));
    }
}
