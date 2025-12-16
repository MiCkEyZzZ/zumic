use std::{collections::HashSet, sync::Arc};

use dashmap::DashMap;
use rand::{seq::IteratorRandom, thread_rng};

use crate::{GeoPoint, GeoSet, Sds, Storage, StoreError, StoreResult, Value};

/// Потокобезопасное in-memory хранилище ключ-значение.
#[derive(Debug)]
pub struct InMemoryStore {
    #[allow(clippy::arc_with_non_send_sync)]
    pub data: Arc<DashMap<Sds, Value>>,
    // GEO-ключи → GeoSet
    #[allow(clippy::arc_with_non_send_sync)]
    geo: Arc<DashMap<Sds, GeoSet>>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl InMemoryStore {
    /// Создаёт новый, пустой `InMemoryStore`.
    /// # Возвращает
    /// - новый экземпляр `InMemoryStore` с инициализированными
    /// - in-memory хранилищами данных и гео-индексов
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
    /// # Возвращает
    /// - итератор, выдающий клонированные пары `(Sds, Value)` для всех
    ///   элементов, находящихся в хранилище
    pub fn iter(&self) -> impl Iterator<Item = (Sds, Value)> + '_ {
        self.data
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
    }
}

impl Storage for InMemoryStore {
    /// Устанавливает значение для указанного ключа.
    ///
    /// # Возвращает:
    /// - `Ok(())` при успешной установке значения
    fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()> {
        self.data.insert(key.clone(), value);
        Ok(())
    }

    /// Получает значение по указанному ключу.
    ///
    /// # Возвращает:
    /// - `Ok(Some(Value))`, если ключ существует
    /// - `Ok(None)`, если ключ отсутствует
    fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>> {
        Ok(self.data.get(key).map(|entry| entry.value().clone()))
    }

    /// Удаляет значение по указанному ключу.
    ///
    /// # Возвращает:
    /// - `Ok(true)`, если ключ существовал и был удалён
    /// - `Ok(false)`, если ключ не существовал
    fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool> {
        Ok(self.data.remove(key).is_some())
    }

    /// Массово устанавливает значения по ключам.
    ///
    /// # Возвращает:
    /// - `Ok(())` после успешной установки всех переданных пар
    fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()> {
        for (key, value) in entries {
            self.data.insert(key.clone(), value);
        }
        Ok(())
    }

    /// Массово получает значения по указанным ключам.
    ///
    /// # Возвращает:
    /// - вектор `Option<Value>`, соответствующий порядку переданных ключей
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

    /// Переименовывает существующий ключ.
    ///
    /// # Возвращает:
    /// - `Ok(())`, если ключ успешно переименован
    /// - ошибку `KeyNotFound`, если исходный ключ не существует
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
    /// # Возвращает:
    /// - `Ok(true)`, если переименование выполнено успешно
    /// - `Ok(false)`, если целевой ключ уже существует
    /// - ошибку `KeyNotFound`, если исходный ключ отсутствует
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
    ///
    /// # Возвращает:
    /// - `Ok(())` после успешной очистки
    fn flushdb(&self) -> StoreResult<()> {
        self.data.clear();
        Ok(())
    }

    /// Добавляет участника с координатами в гео-набор по ключу.
    ///
    /// # Возвращает:
    /// - `Ok(true)`, если участник добавлен впервые
    /// - `Ok(false)`, если участник уже существовал
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

    /// Вычисляет расстояние между двумя участниками в гео-наборе.
    ///
    /// # Возвращает:
    /// - `Ok(Some(dist))`, если оба участника найдены
    /// - `Ok(None)`, если один из участников отсутствует
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

    /// Возвращает координаты участника из гео-набора.
    ///
    /// # Возвращает:
    /// - `Ok(Some(GeoPoint))`, если участник найден
    /// - `Ok(None)`, если участник отсутствует
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

    /// Возвращает всех участников гео-набора, находящихся в радиусе от заданной
    /// точки.
    ///
    /// # Возвращает:
    /// - список кортежей `(member, distance, GeoPoint)`
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
        // 1. перевести в метры
        let radius_m = match unit {
            "km" => radius * 1000.0,
            "mi" => radius * 1609.344,
            "ft" => radius / 3.28084,
            _ => radius, // assume meters
        };
        // фильтр в метрах
        let mut raw = set.radius(lon, lat, radius_m);
        // 2. преобразуйте обратно для вывода
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

    /// Возвращает участников, находящихся в радиусе от указанного участника.
    ///
    /// # Возвращает:
    /// - список кортежей `(member, distance, GeoPoint)` от исходного участника
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

    /// Добавляет элементы в множество.
    ///
    /// # Возвращает:
    /// - количество реально добавленных элементов
    fn sadd(
        &self,
        key: &Sds,
        members: &[Sds],
    ) -> StoreResult<usize> {
        // Попробуем получить мутируемый доступ, если ключ уже существует
        if let Some(mut entry) = self.data.get_mut(key) {
            match &mut *entry {
                Value::Set(set) => {
                    let mut added = 0usize;
                    for m in members {
                        if set.insert(m.clone()) {
                            added += 1;
                        }
                    }
                    return Ok(added);
                }
                _ => return Err(StoreError::WrongType("SADD: key is not a set".into())),
            }
        }

        // Если ключа нет — создаём новое множество
        let mut set = HashSet::with_capacity(members.len());
        let mut added = 0usize;
        for m in members {
            if set.insert(m.clone()) {
                added += 1;
            }
        }
        self.data.insert(key.clone(), Value::Set(set));
        Ok(added)
    }

    /// Возвращает все элементы множества.
    ///
    /// # Возвращает:
    /// - вектор элементов множества
    fn smembers(
        &self,
        key: &Sds,
    ) -> StoreResult<Vec<Sds>> {
        match self.data.get(key) {
            Some(entry) => match &*entry {
                Value::Set(set) => Ok(set.iter().cloned().collect()),
                _ => Err(StoreError::WrongType("SMEMBERS: key is not a set".into())),
            },
            None => Ok(Vec::new()),
        }
    }

    /// Возвращает размер множества.
    ///
    /// # Возвращает:
    /// - количество элементов множества
    fn scard(
        &self,
        key: &Sds,
    ) -> StoreResult<usize> {
        match self.data.get(key) {
            Some(entry) => match &*entry {
                Value::Set(set) => Ok(set.len()),
                _ => Err(StoreError::WrongType("SCARD: key is not a set".into())),
            },
            None => Ok(0),
        }
    }

    /// Проверяет, является ли элемент членом множества.
    ///
    /// # Возвращает:
    /// - `true`, если элемент присутствует
    /// - `false`, если отсутствует
    fn sismember(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<bool> {
        match self.data.get(key) {
            Some(entry) => match &*entry {
                Value::Set(set) => Ok(set.contains(member)),
                _ => Err(StoreError::WrongType("SISMEMBER: key is not a set".into())),
            },
            None => Ok(false),
        }
    }

    /// Удаляет элементы из множества.
    ///
    /// # Возвращает:
    /// - количество реально удалённых элементов
    fn srem(
        &self,
        key: &Sds,
        members: &[Sds],
    ) -> StoreResult<usize> {
        let mut removed = 0usize;
        let mut remove_key = false;

        if let Some(mut entry) = self.data.get_mut(key) {
            match &mut *entry {
                Value::Set(set) => {
                    for m in members {
                        if set.remove(m) {
                            removed += 1;
                        }
                    }
                    if set.is_empty() {
                        // не удаляем сразу — выставляем флаг и удалим после выхода из блока
                        remove_key = true;
                    }
                }
                _ => return Err(StoreError::WrongType("SREM: key is not a set".into())),
            }
            // entry dropped here (end of this scope)
        } else {
            return Ok(0);
        }

        if remove_key {
            // безопасно удалить, так как entry уже вышел из области видимости
            self.data.remove(key);
        }

        Ok(removed)
    }

    /// Возвращает случайные элементы множества.
    ///
    /// # Возвращает:
    /// - вектор выбранных элементов (может быть пустым)
    fn srandmember(
        &self,
        key: &Sds,
        count: isize,
    ) -> StoreResult<Vec<Sds>> {
        match self.data.get(key) {
            Some(entry) => match &*entry {
                Value::Set(set) => {
                    let mut rng = thread_rng();
                    if count == 1 {
                        let opt = set.iter().cloned().choose(&mut rng);
                        Ok(opt.into_iter().collect())
                    } else if count > 1 {
                        let cnt = count as usize;
                        let chosen = set.iter().cloned().choose_multiple(&mut rng, cnt);
                        Ok(chosen)
                    } else {
                        // count <= 0 semantics: возвращаем пустой вектор
                        Ok(Vec::new())
                    }
                }
                _ => Err(StoreError::WrongType(
                    "SRANDMEMBER: key is not a set".into(),
                )),
            },
            None => Ok(Vec::new()),
        }
    }

    /// Извлекает и удаляет элементы множества случайным образом.
    ///
    /// # Возвращает:
    /// - вектор удалённых элементов
    fn spop(
        &self,
        key: &Sds,
        count: isize,
    ) -> StoreResult<Vec<Sds>> {
        let mut out = Vec::new();
        let mut remove_key = false;

        if let Some(mut entry) = self.data.get_mut(key) {
            match &mut *entry {
                Value::Set(set) => {
                    let mut rng = thread_rng();
                    let cnt = if count <= 0 { 1 } else { count as usize };
                    for _ in 0..cnt {
                        if let Some(item) = set.iter().cloned().choose(&mut rng) {
                            set.remove(&item);
                            out.push(item);
                        } else {
                            break;
                        }
                    }
                    if set.is_empty() {
                        remove_key = true;
                    }
                }
                _ => return Err(StoreError::WrongType("SPOP: key is not a set".into())),
            }
            // entry dropped here
        } else {
            return Ok(Vec::new());
        }

        if remove_key {
            self.data.remove(key);
        }

        Ok(out)
    }

    /// Возвращает количество ключей в базе.
    ///
    /// # Возвращает:
    /// - количество ключей
    fn dbsize(&self) -> StoreResult<usize> {
        Ok(self.data.len())
    }

    /// Сохранение не поддерживается для in-memory хранилища.
    ///
    /// # Возвращает:
    /// - ошибку `UnsupportedOperation`
    fn save(&self) -> StoreResult<()> {
        // In-memory store не поддерживает персистентность
        Err(StoreError::UnsupportedOperation(
            "SAVE not supported for in-memory storage".into(),
        ))
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для InMemoryStore
////////////////////////////////////////////////////////////////////////////////

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    fn key(data: &str) -> Sds {
        Sds::from(data.as_bytes())
    }

    /// Тест: установка и получение значения.
    /// Проверяет, что после `set` значение можно забрать через `get`.
    #[test]
    fn test_set_and_get() {
        let store = InMemoryStore::new();
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));

        store.set(&k, v.clone()).unwrap();
        let got = store.get(&k).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Тест: перезапись значения.
    /// Проверяет, что второй `set` по тому же ключу обновляет значение.
    #[test]
    fn test_overwrite_value() {
        let store = InMemoryStore::new();
        let k = key("overwrite");

        store.set(&k, Value::Str(Sds::from_str("one"))).unwrap();
        store.set(&k, Value::Str(Sds::from_str("two"))).unwrap();

        let got = store.get(&k).unwrap();
        assert_eq!(got, Some(Value::Str(Sds::from_str("two"))));
    }

    /// Тест: удаление ключа.
    /// Проверяет, что `del` удаляет ключ и `get` возвращает `None`.
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

    /// Тест: получение несуществующего ключа.
    /// Проверяет, что `get` на отсутствии возвращает `None`.
    #[test]
    fn test_get_nonexistent_key() {
        let store = InMemoryStore::new();
        let got = store.get(&key("missing")).unwrap();
        assert_eq!(got, None);
    }

    /// Тест: удаление несуществующего ключа.
    /// Проверяет, что `del` на отсутствии не падает.
    #[test]
    fn test_delete_nonexistent_key() {
        let store = InMemoryStore::new();
        // Deleting a non-existent key should not cause an error.
        assert!(store.del(&key("nope")).is_ok());
    }

    /// Тест: множественная вставка и получение.
    /// Проверяет `mset` и `mget` для нескольких пар.
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

    /// Тест: переименование ключа.
    /// Проверяет, что `rename` меняет имя ключа.
    #[test]
    fn test_rename() {
        let store = InMemoryStore::new();
        store.set(&key("old"), Value::Int(123)).unwrap();

        store.rename(&key("old"), &key("new")).unwrap();
        assert!(store.get(&key("old")).unwrap().is_none());
        assert_eq!(store.get(&key("new")).unwrap(), Some(Value::Int(123)));
    }

    /// Тест: rename несуществующего ключа.
    /// Проверяет, что `rename` возвращает ошибку `KeyNotFound`.
    #[test]
    fn test_rename_nonexistent_key() {
        let store = InMemoryStore::new();
        let result = store.rename(&key("does_not_exist"), &key("whatever"));
        assert!(matches!(result, Err(StoreError::KeyNotFound)));
    }

    /// Тест: renamenx успешен.
    /// Проверяет, что `renamenx` переименовывает, если целевой ключ
    /// отсутствует.
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

    /// Тест: renamenx при существующем целевом ключе.
    /// Проверяет, что `renamenx` возвращает `false`.
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

    /// Тест: flushdb очищает всё.
    /// Проверяет, что после `flushdb` база пуста.
    #[test]
    fn test_flushdb() {
        let store = InMemoryStore::new();
        store.set(&key("one"), Value::Int(1)).unwrap();
        store.set(&key("two"), Value::Int(2)).unwrap();

        store.flushdb().unwrap();

        assert!(store.get(&key("one")).unwrap().is_none());
        assert!(store.get(&key("two")).unwrap().is_none());
    }

    /// Тест: пустой ключ.
    /// Проверяет, что можно использовать пустую строку как ключ.
    #[test]
    fn test_empty_key() {
        let store = InMemoryStore::new();
        let empty = key("");
        store.set(&empty, Value::Int(42)).unwrap();
        assert_eq!(store.get(&empty).unwrap(), Some(Value::Int(42)));
    }

    /// Тест: длинные ключи и значения.
    /// Проверяет работу с большими данными.
    #[test]
    fn test_very_long_key_and_value() {
        let store = InMemoryStore::new();
        let long_key = key(&"k".repeat(10_000));
        let long_value = Value::Str(Sds::from("v".repeat(100_000).as_bytes()));

        store.set(&long_key, long_value.clone()).unwrap();
        assert_eq!(store.get(&long_key).unwrap(), Some(long_value));
    }

    /// Тест: geo_add и geo_pos.
    /// Проверяет добавление точки и получение её координат.
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

    /// Тест: geo_dist.
    /// Проверяет расстояние между Парижем и Берлином.
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

    /// Тест: geo_radius вокруг точки.
    /// Проверяет, что возвращаются точки внутри заданного радиуса.
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
        let members: Vec<_> = results.iter().map(|(m, ..)| m.clone()).collect();

        assert!(members.contains(&"center".to_string()));
        assert!(members.contains(&"near".to_string()));
        assert!(!members.contains(&"far".to_string()));
    }

    /// Тест: geo_radius_by_member вокруг участника.
    /// Проверяет, что возвращаются соседи внутри радиуса вокруг origin.
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
        let members: Vec<_> = results.iter().map(|(m, ..)| m.clone()).collect();

        assert!(members.contains(&"origin".to_string()));
        assert!(members.contains(&"east".to_string()));
        assert!(members.contains(&"north".to_string()));
        assert!(!members.contains(&"faraway".to_string()));
    }
}
