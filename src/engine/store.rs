use std::io::{self};

use super::{InClusterStore, InMemoryStore, InPersistentStore};
use crate::{
    config::settings::{StorageConfig, StorageType},
    Sds, Storage, StoreResult, Value,
};

/// Перечисление движков хранилища данных.
/// Может представлять хранилище в памяти, кластерное хранилище
/// или персистентное (дисковое).
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

    /// Возвращает ссылку на конкретное хранилище, реализующее трейд `Storage`.
    pub fn get_store(&self) -> &dyn Storage {
        match self {
            Self::Memory(store) => store,
            Self::Cluster(store) => store,
            Self::Persistent(store) => store,
        }
    }

    /// Возвращает изменяемую ссылку на конкретное хранилище, реализующее трейд `Storage`.
    pub fn get_store_mut(&mut self) -> &mut dyn Storage {
        match self {
            Self::Memory(store) => store,
            Self::Cluster(store) => store,
            Self::Persistent(store) => store,
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
}
