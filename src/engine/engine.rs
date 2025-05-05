use std::io::{self};

use tracing::info;

use crate::{
    config::settings::{StorageConfig, StorageType},
    Sds, Storage, StoreResult, Value,
};

use super::InMemoryStore;

pub enum StorageEngine {
    InMemory(InMemoryStore),
}

impl StorageEngine {
    pub fn set(&self, key: &Sds, value: Value) -> StoreResult<()> {
        info!("Setting value for key: {:?}", key);
        match self {
            StorageEngine::InMemory(store) => store.set(key, value),
        }
    }

    pub fn get(&self, key: &Sds) -> StoreResult<Option<Value>> {
        info!("Getting value for key: {:?}", key);
        match self {
            StorageEngine::InMemory(store) => store.get(key),
        }
    }

    pub fn del(&self, key: &Sds) -> StoreResult<i64> {
        info!("Deleting key: {:?}", key);
        match self {
            StorageEngine::InMemory(store) => store.del(key),
        }
    }

    pub fn mset(&self, entries: Vec<(&Sds, Value)>) -> StoreResult<()> {
        info!("MSET {} leys", entries.len());
        match self {
            StorageEngine::InMemory(store) => store.mset(entries),
        }
    }

    pub fn mget(&self, keys: &[&Sds]) -> StoreResult<Vec<Option<Value>>> {
        info!("MGET {} keys", keys.len());
        match self {
            StorageEngine::InMemory(store) => store.mget(keys),
        }
    }

    pub fn rename(&self, from: &Sds, to: &Sds) -> StoreResult<()> {
        info!("Renaming key: {:?} to {:?}", from, to);
        match self {
            StorageEngine::InMemory(store) => store.rename(from, to),
        }
    }

    pub fn renamenx(&self, from: &Sds, to: &Sds) -> StoreResult<bool> {
        info!("Renaming key (NX): {:?} to {:?}", from, to);
        match self {
            StorageEngine::InMemory(store) => store.renamenx(from, to),
        }
    }

    pub fn flushdb(&self) -> StoreResult<()> {
        info!("Flushing database");
        match self {
            StorageEngine::InMemory(store) => store.flushdb(),
        }
    }
    /// Инициализирует движок хранения на основе переданной конфигурации.
    pub fn initialize(config: &StorageConfig) -> io::Result<Self> {
        match &config.storage_type {
            StorageType::Memory => Ok(Self::InMemory(InMemoryStore::new())),
        }
    }

    /// Получает ссылку на конкретное хранилище через общий трейт `Storage`
    pub fn get_store(&self) -> &dyn Storage {
        match self {
            Self::InMemory(store) => store,
        }
    }
    pub fn get_store_mut(&mut self) -> &mut dyn Storage {
        match self {
            Self::InMemory(store) => store,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(data: &str) -> Sds {
        Sds::from(data.as_bytes())
    }

    /// Тестирует, что установка значения, а затем его получение возвращают то же значение.
    #[test]
    fn test_engine_set_and_get() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("foo");
        let v = Value::Str(Sds::from_str("bar"));

        engine.set(&k, v.clone()).unwrap();
        let got = engine.get(&k).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Проверяет, что получение значения по несуществующему ключу возвращает None.
    #[test]
    fn test_engine_get_nonexistent_key() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("not_found");

        let got = engine.get(&k).unwrap();
        assert_eq!(got, None);
    }

    /// Проверяет, что удаление существующего ключа удаляет его из хранилища.
    #[test]
    fn test_engine_delete() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));

        engine.set(&k, v).unwrap();
        engine.del(&k).unwrap();

        let got = engine.get(&k).unwrap();
        assert_eq!(got, None)
    }

    /// Проверяет, что удаление несуществующего ключа не приводит к ошибке.
    #[test]
    fn test_engine_delete_nonexistent_key() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("ghost");

        // Удаление не должно вызывать панику или ошибку.
        let result = engine.del(&k);
        assert!(result.is_ok());
    }

    /// Тестирует установку нескольких пар ключ-значение с помощью mset.
    #[test]
    fn test_engine_mset() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());

        // Живые переменные, чтобы ссылки были валидны до конца функции
        let k1 = key("kin1");
        let k2 = key("kin2");
        let v1 = Value::Str(Sds::from_str("dza1"));
        let v2 = Value::Str(Sds::from_str("dza2"));

        // Собираем Vec<(&Sds, Value)>
        let entries = vec![(&k1, v1.clone()), (&k2, v2.clone())];
        engine.mset(entries).unwrap();

        // Проверяем, что положилось
        assert_eq!(engine.get(&k1).unwrap(), Some(v1));
        assert_eq!(engine.get(&k2).unwrap(), Some(v2));
    }

    /// Проверяет, что mget возвращает значения в корректном порядке для нескольких ключей.
    #[test]
    fn test_engine_mget() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("kin1");
        let k2 = key("kin2");
        let v1 = Value::Str(Sds::from_str("dza1"));
        let v2 = Value::Str(Sds::from_str("dza2"));

        engine.set(&k1, v1.clone()).unwrap();
        engine.set(&k2, v2.clone()).unwrap();

        let got = engine.mget(&[&k1, &k2]).unwrap();
        assert_eq!(got, vec![Some(v1), Some(v2)]);
    }

    /// Проверяет, что ключ успешно переименовывается.
    #[test]
    fn test_engine_rename() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(Sds::from_str("value"));

        engine.set(&k1, v.clone()).unwrap();
        engine.rename(&k1, &k2).unwrap();

        let got = engine.get(&k2).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Проверяет, что переименование несуществующего ключа приводит к ошибке.
    #[test]
    fn test_engine_rename_nonexistent_key() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");

        // Должна возвращаться ошибка при попытке переименовать несуществующий ключ.
        let result = engine.rename(&k1, &k2);
        assert!(result.is_err());
    }

    /// Тестирует поведение метода renamenx: переименование выполняется, только если новый
    /// ключ отсутствует.
    #[test]
    fn test_engine_renamenx() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(Sds::from_str("value"));

        engine.set(&k1, v.clone()).unwrap();
        let result = engine.renamenx(&k1, &k2).unwrap();
        assert!(result);

        // Проверяем, что старый ключ удалён, а новый присутствует.
        let got = engine.get(&k2).unwrap();
        assert_eq!(got, Some(v));

        // Повторная попытка переименования должна не выполниться, так как новый ключ уже существует.
        let result = engine.renamenx(&k1, &k2).unwrap();
        assert!(!result);
    }

    /// Тестирует, что flushdb очищает все данные из хранилища.
    #[test]
    fn test_engine_flushdb() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
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

    /// Тестирует инициализацию движка с конфигурацией памяти.
    #[test]
    fn test_engine_initialize_memory() {
        let config = StorageConfig {
            storage_type: StorageType::Memory,
        };

        let engine = StorageEngine::initialize(&config);
        assert!(engine.is_ok());
    }

    /// Тестирует, что метод get_store возвращает объект-трейт,
    /// с которым можно работать.
    #[test]
    fn test_engine_get_store() {
        let engine = StorageEngine::InMemory(InMemoryStore::new());
        let store = engine.get_store();
        assert!(store.mget(&[]).is_ok());
    }

    /// Тестирует, что get_store_mut возвращает изменяемый объект-трейт,
    /// с которым можно работать.
    #[test]
    fn test_engine_get_store_mut() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let store_mut = engine.get_store_mut();
        assert!(store_mut.set(&key("x"), Value::Int(42)).is_ok());

        let got = store_mut.get(&key("x")).unwrap();
        assert_eq!(got, Some(Value::Int(42)));
    }
}
