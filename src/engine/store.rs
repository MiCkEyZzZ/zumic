use std::io::{self};

use super::{InMemoryStore, InPersistentStore};
use crate::{
    config::settings::{StorageConfig, StorageType},
    engine::cluster::InClusterStore,
    GeoPoint, Sds, Storage, StoreResult, Value,
};

/// Координата для географических данных.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoCoord {
    pub longitude: f64,
    pub latitude: f64,
}

/// Перечисление движков хранилища данных.
/// Может представлять хранилище в памяти, кластерное хранилище
/// или персистентное (дисковое).
#[allow(clippy::large_enum_variant)]
pub enum StorageEngine {
    /// Хранилище в памяти
    Memory(InMemoryStore),
    /// Кластерное хранилище (распределённое)
    Cluster(InClusterStore),
    /// Персистентное хранилище на диске
    Persistent(InPersistentStore),
}

impl StorageEngine {
    /// Устанавливает значение по ключу в выбранном движке хранения.
    /// Если значение уже существует, оно будет перезаписано.
    pub fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.set(key, value),
            StorageEngine::Cluster(store) => store.set(key, value),
            StorageEngine::Persistent(store) => store.set(key, value),
        }
    }

    /// Получает значение по ключу.
    /// Если ключ отсутствует, возвращает `None`.
    pub fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>> {
        match self {
            StorageEngine::Memory(store) => store.get(key),
            StorageEngine::Cluster(store) => store.get(key),
            StorageEngine::Persistent(store) => store.get(key),
        }
    }

    /// Удаляет ключ из хранилища.
    ///
    /// Возвращает `true`, если ключ был удалён, `false` если ключ не существовал.
    pub fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool> {
        match self {
            StorageEngine::Memory(store) => store.del(key),
            StorageEngine::Cluster(store) => store.del(key),
            StorageEngine::Persistent(store) => store.del(key),
        }
    }

    /// Устанавливает несколько пар ключ-значение за одну операцию.
    pub fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.mset(entries),
            StorageEngine::Cluster(store) => store.mset(entries),
            StorageEngine::Persistent(store) => store.mset(entries),
        }
    }

    /// Получает значения для списка ключей.
    /// Для отсутствующих ключей возвращает `None`.
    pub fn mget(
        &self,
        keys: &[&Sds],
    ) -> StoreResult<Vec<Option<Value>>> {
        match self {
            StorageEngine::Memory(store) => store.mget(keys),
            StorageEngine::Cluster(store) => store.mget(keys),
            StorageEngine::Persistent(store) => store.mget(keys),
        }
    }

    /// Переименовывает ключ `from` в `to`.
    ///
    /// Возвращает ошибку, если ключ `from` не существует.
    pub fn rename(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.rename(from, to),
            StorageEngine::Cluster(store) => store.rename(from, to),
            StorageEngine::Persistent(store) => store.rename(from, to),
        }
    }

    /// Переименовывает ключ `from` в `to` только если ключ `to` ещё не существует.
    ///
    /// Возвращает `true` если переименование прошло успешно,
    /// `false` если ключ `to` уже существует.
    pub fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool> {
        match self {
            StorageEngine::Memory(store) => store.renamenx(from, to),
            StorageEngine::Cluster(store) => store.renamenx(from, to),
            StorageEngine::Persistent(store) => store.renamenx(from, to),
        }
    }

    /// Очищает всю базу данных, удаляя все ключи.
    pub fn flushdb(&self) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.flushdb(),
            StorageEngine::Cluster(store) => store.flushdb(),
            StorageEngine::Persistent(store) => store.flushdb(),
        }
    }

    /// Инициализирует движок хранения на основе конфигурации.
    ///
    /// Возвращает ошибку ввода-вывода в случае неудачи.
    pub fn initialize(config: &StorageConfig) -> io::Result<Self> {
        match &config.storage_type {
            StorageType::Memory => Ok(Self::Memory(InMemoryStore::new())),
            StorageType::Cluster => todo!("Cluster store initialization"),
            StorageType::Persistent => todo!("Persistent store initialization"),
        }
    }

    /// Возвращает ссылку на конкретное хранилище, реализующее трейт `Storage`.
    pub fn get_store(&self) -> &dyn Storage {
        match self {
            Self::Memory(store) => store,
            Self::Cluster(store) => store,
            Self::Persistent(store) => store,
        }
    }

    /// Возвращает изменяемую ссылку на конкретное хранилище, реализующее трейт `Storage`.
    pub fn get_store_mut(&mut self) -> &mut dyn Storage {
        match self {
            Self::Memory(store) => store,
            Self::Cluster(store) => store,
            Self::Persistent(store) => store,
        }
    }

    /// Добавляет точку `(lon, lat)` с именем `member` в гео-набор под ключом `key`.
    ///
    /// Возвращает `Ok(true)`, если `member` был добавлен впервые,
    /// и `Ok(false)`, если он уже присутствовал.
    pub fn geo_add(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        member: &Sds,
    ) -> StoreResult<bool> {
        match self {
            StorageEngine::Memory(store) => store.geo_add(key, lon, lat, member),
            StorageEngine::Cluster(store) => store.geo_add(key, lon, lat, member),
            StorageEngine::Persistent(store) => store.geo_add(key, lon, lat, member),
        }
    }

    /// Возвращает расстояние между `member1` и `member2` в единицах `unit`:
    /// `"m"` (метры), `"km"`, `"mi"`, `"ft"`.
    ///
    /// Если один из членов отсутствует — возвращает `Ok(None)`.
    pub fn geo_dist(
        &self,
        key: &Sds,
        member1: &Sds,
        member2: &Sds,
        unit: &str,
    ) -> StoreResult<Option<f64>> {
        match self {
            StorageEngine::Memory(store) => store.geo_dist(key, member1, member2, unit),
            StorageEngine::Cluster(store) => store.geo_dist(key, member1, member2, unit),
            StorageEngine::Persistent(store) => store.geo_dist(key, member1, member2, unit),
        }
    }

    /// Возвращает координаты `(lon, lat)` для данного `member`,
    /// или `Ok(None)`, если его нет.
    pub fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>> {
        match self {
            StorageEngine::Memory(store) => store.geo_pos(key, member),
            StorageEngine::Cluster(store) => store.geo_pos(key, member),
            StorageEngine::Persistent(store) => store.geo_pos(key, member),
        }
    }

    /// Ищет всех членов в радиусе `radius` от точки `(lon, lat)`.
    ///
    /// `unit` может быть `"m"`, `"km"`, `"mi"`, `"ft"`.
    ///
    /// Возвращает массив кортежей `(member, distance, GeoPoint)`.
    pub fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        match self {
            StorageEngine::Memory(store) => store.geo_radius_by_member(key, member, radius, unit),
            StorageEngine::Cluster(store) => store.geo_radius_by_member(key, member, radius, unit),
            StorageEngine::Persistent(store) => {
                store.geo_radius_by_member(key, member, radius, unit)
            }
        }
    }

    /// То же, что `geo_radius`, но центр задаётся координатами уже существующего `member`.
    ///
    /// Если `member` не найден — возвращает пустой вектор.
    pub fn geo_radius(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>> {
        match self {
            StorageEngine::Memory(store) => store.geo_radius(key, lon, lat, radius, unit),
            StorageEngine::Cluster(store) => store.geo_radius(key, lon, lat, radius, unit),
            StorageEngine::Persistent(store) => store.geo_radius(key, lon, lat, radius, unit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(data: &str) -> Sds {
        Sds::from(data.as_bytes())
    }

    /// Тест проверяет установку значения и последующее получение должно вернуть то же значение.
    #[test]
    fn test_engine_set_and_get() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("kin");
        let v = Value::Str(Sds::from_str("dzadza"));
        engine.set(&k, v.clone()).unwrap();
        let got = engine.get(&k).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Тест проверяет получение по несуществующему ключу возвращает None.
    #[test]
    fn test_engine_get_nonexistent_key() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("not_found");
        let got = engine.get(&k).unwrap();
        assert_eq!(got, None);
    }

    /// Тест проверяет удаление существующего ключа должно удалить ключ.
    #[test]
    fn test_engine_delete() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));
        engine.set(&k, v).unwrap();
        engine.del(&k).unwrap();
        let got = engine.get(&k).unwrap();
        assert_eq!(got, None)
    }

    /// Тест проверяет удаление несуществующего ключа не должно вызывать ошибку.
    #[test]
    fn test_engine_delete_nonexistent_key() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("ghost");
        // Удаление не должно вызывать паники или ошибки.
        let result = engine.del(&k);
        assert!(result.is_ok());
    }

    /// Тест проверяет установка нескольких пар ключ-значение с помощью mset.
    #[test]
    fn test_engine_mset() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        // Текущие переменные, поэтому ссылки действительны до конца работы функции.
        let k1 = key("kin1");
        let k2 = key("kin2");
        let v1 = Value::Str(Sds::from_str("dza1"));
        let v2 = Value::Str(Sds::from_str("dza2"));
        // Собирать Vec<(&Sds, значение)>
        let entries = vec![(&k1, v1.clone()), (&k2, v2.clone())];
        engine.mset(entries).unwrap();
        // Проверяю, что должно было быть сделано
        assert_eq!(engine.get(&k1).unwrap(), Some(v1));
        assert_eq!(engine.get(&k2).unwrap(), Some(v2));
    }

    /// Тест проверяет получение нескольких значений с помощью mget возвращает их в правильном порядке.
    #[test]
    fn test_engine_mget() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("kin1");
        let k2 = key("kin2");
        let v1 = Value::Str(Sds::from_str("dza1"));
        let v2 = Value::Str(Sds::from_str("dza2"));
        engine.set(&k1, v1.clone()).unwrap();
        engine.set(&k2, v2.clone()).unwrap();
        let got = engine.mget(&[&k1, &k2]).unwrap();
        assert_eq!(got, vec![Some(v1), Some(v2)]);
    }

    /// Тест проверяет успешное переименование ключа.
    #[test]
    fn test_engine_rename() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(Sds::from_str("value"));
        engine.set(&k1, v.clone()).unwrap();
        engine.rename(&k1, &k2).unwrap();
        let got = engine.get(&k2).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Тест проверяет попытку переименовать несуществующий ключ должна вернуть ошибку.
    #[test]
    fn test_engine_rename_nonexistent_key() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        // При попытке переименовать несуществующий ключ должна быть возвращена ошибка.
        let result = engine.rename(&k1, &k2);
        assert!(result.is_err());
    }

    /// Тест проверяет renamenx переименовывает ключ только если новый ключ отсутствует.
    #[test]
    fn test_engine_renamenx() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(Sds::from_str("value"));
        engine.set(&k1, v.clone()).unwrap();
        let result = engine.renamenx(&k1, &k2).unwrap();
        assert!(result);
        // Убедитесь, что старый ключ удален, а новый присутствует.
        let got = engine.get(&k2).unwrap();
        assert_eq!(got, Some(v));
        // // Повторная попытка переименования должна завершиться неудачей, поскольку новый ключ уже существует.
        let result = engine.renamenx(&k1, &k2).unwrap();
        assert!(!result);
    }

    /// Тест проверяет, что `flushdb` очищает все данные из хранилища.
    #[test]
    fn test_engine_flushdb() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        engine
            .set(&key("a"), Value::Str(Sds::from_str("x")))
            .unwrap();
        engine
            .set(&key("b"), Value::Str(Sds::from_str("y")))
            .unwrap();
        engine.flushdb().unwrap();
        let a = engine.get(&key("a")).unwrap();
        let b = engine.get(&key("b")).unwrap();
        assert_eq!(a, None);
        assert_eq!(b, None);
    }

    /// Тест проверяет инициализацию движка с конфигурацией памяти.
    #[test]
    fn test_engine_initialize_memory() {
        let config = StorageConfig {
            storage_type: StorageType::Memory,
        };
        let engine = StorageEngine::initialize(&config);
        assert!(engine.is_ok());
    }

    /// Тест проверяет, что `get_store` возвращает объект трейта, с которым можно работать.
    #[test]
    fn test_engine_get_store() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let store = engine.get_store();
        assert!(store.mget(&[]).is_ok());
    }

    /// Тест проверяет, что `get_store_mut` возвращает изменяемый объект трейта.
    #[test]
    fn test_engine_get_store_mut() {
        let mut engine = StorageEngine::Memory(InMemoryStore::new());
        let store_mut = engine.get_store_mut();
        assert!(store_mut.set(&key("x"), Value::Int(42)).is_ok());
        let got = store_mut.get(&key("x")).unwrap();
        assert_eq!(got, Some(Value::Int(42)));
    }

    /// Тестирует geo_add и geo_pos: добавление точки и получение её координат.
    #[test]
    fn test_engine_geo_add_and_pos() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let paris_key = key("cities");
        let paris = key("paris");

        // Добавляем координаты.
        let added = engine.geo_add(&paris_key, 2.3522, 48.8566, &paris).unwrap();
        assert!(added, "Первое добавление должно вернуть true");
        let added_again = engine.geo_add(&paris_key, 2.3522, 48.8566, &paris).unwrap();
        assert!(
            !added_again,
            "Повторное добавление того же члена должно вернуть false"
        );

        // Позиция
        let pos = engine.geo_pos(&paris_key, &paris).unwrap().unwrap();
        assert!((pos.lon - 2.3522).abs() < 1e-6);
        assert!((pos.lat - 48.8566).abs() < 1e-6);
    }

    /// Тестирует geo_dist: вычисление расстояния между двумя точками.
    #[test]
    fn test_engine_geo_dist() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let cities_key = key("cities");
        let a = key("a"); // пусть это Париж
        let b = key("b"); // пусть это Берлин

        engine.geo_add(&cities_key, 2.3522, 48.8566, &a).unwrap();
        engine.geo_add(&cities_key, 13.4050, 52.5200, &b).unwrap();

        // Расстояние в километрах
        let d_km = engine.geo_dist(&cities_key, &a, &b, "km").unwrap().unwrap();
        assert!((d_km - 878.0).abs() < 10.0);

        // Расстояние в метрах
        let d_m = engine.geo_dist(&cities_key, &a, &b, "m").unwrap().unwrap();
        assert!((d_m - 878_000.0).abs() < 20_000.0);
    }

    /// Тестирует geo_radius: поиск точек в радиусе от координаты.
    #[test]
    fn test_engine_geo_radius() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let landmarks_key = key("landmarks");

        engine
            .geo_add(&landmarks_key, 0.0, 0.0, &key("center"))
            .unwrap();
        engine
            .geo_add(&landmarks_key, 0.001, 0.001, &key("near"))
            .unwrap();
        engine
            .geo_add(&landmarks_key, 10.0, 10.0, &key("far"))
            .unwrap();

        // Ищем в радиусе 0.2 km (200 м)
        let res = engine
            .geo_radius(&landmarks_key, 0.0, 0.0, 0.2, "km")
            .unwrap();
        let names: Vec<_> = res.iter().map(|(m, _, _)| m.clone()).collect();

        assert!(names.contains(&"center".to_string()));
        assert!(names.contains(&"near".to_string()));
        assert!(!names.contains(&"far".to_string()));
    }

    /// Тестирует geo_radius_by_member: поиск по координатам заданного члена.
    #[test]
    fn test_engine_geo_radius_by_member() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let points_key = key("points");

        engine
            .geo_add(&points_key, 0.0, 0.0, &key("origin"))
            .unwrap();
        engine
            .geo_add(&points_key, 0.002, 0.0, &key("east"))
            .unwrap();
        engine
            .geo_add(&points_key, 0.0, 0.002, &key("north"))
            .unwrap();
        engine.geo_add(&points_key, 1.0, 1.0, &key("far")).unwrap();

        // Радиус 0.3 km от "origin"
        let res = engine
            .geo_radius_by_member(&points_key, &key("origin"), 0.3, "km")
            .unwrap();
        let names: Vec<_> = res.iter().map(|(m, _, _)| m.clone()).collect();

        assert!(names.contains(&"origin".to_string()));
        assert!(names.contains(&"east".to_string()));
        assert!(names.contains(&"north".to_string()));
        assert!(!names.contains(&"far".to_string()));
    }
}
