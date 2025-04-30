use std::sync::Arc;

use dashmap::DashMap;

use crate::{Sds, StoragePort, StoreError, StoreResult, Value};

/// `InMemoryStore` — потокобезопасное хранилище ключей и значений
/// с использованием `DashMap` и `Arc`.
pub struct InMemoryStore {
    pub data: Arc<DashMap<Sds, Value>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
        }
    }
}

impl StoragePort for InMemoryStore {
    fn set(&mut self, key: Sds, value: Value) -> StoreResult<()> {
        self.data.insert(key, value);
        Ok(())
    }

    fn get(&mut self, key: Sds) -> StoreResult<Option<Value>> {
        Ok(self.data.get(&key).map(|entry| entry.clone()))
    }
    fn del(&self, key: Sds) -> StoreResult<i64> {
        if self.data.remove(&key).is_some() {
            Ok(1)
        } else {
            Ok(0)
        }
    }
    fn mset(&mut self, entries: Vec<(Sds, Value)>) -> StoreResult<()> {
        for (key, value) in entries {
            self.data.insert(key, value);
        }
        Ok(())
    }
    fn mget(&self, keys: &[Sds]) -> StoreResult<Vec<Option<Value>>> {
        let result = keys
            .iter()
            .map(|key| self.data.get(key).map(|entry| entry.clone()))
            .collect();
        Ok(result)
    }
    fn rename(&mut self, from: Sds, to: Sds) -> StoreResult<()> {
        if let Some((_, value)) = self.data.remove(&from) {
            self.data.insert(to, value);
            Ok(())
        } else {
            Err(StoreError::KeyNotFound)
        }
    }
    fn renamenx(&mut self, from: Sds, to: Sds) -> StoreResult<bool> {
        if self.data.contains_key(&to) {
            return Ok(false);
        }
        if let Some((_, value)) = self.data.remove(&from) {
            self.data.insert(to, value);
            Ok(true)
        } else {
            Err(StoreError::KeyNotFound)
        }
    }
    fn flushdb(&mut self) -> StoreResult<()> {
        self.data.clear();
        Ok(())
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

    /// Основной тест для проверки установки и последующего получения значения.
    #[test]
    fn test_set_and_get() {
        let mut store = InMemoryStore::new();
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));

        store.set(k.clone(), v.clone()).unwrap();
        let got = store.get(k.clone()).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Проверяет, что повторная установка значения для одного и того же ключа
    /// перезаписывает старое значение.
    #[test]
    fn test_overwrite_value() {
        let mut store = InMemoryStore::new();
        let k = key("overwrite");
        let v1 = Value::Str(Sds::from_str("one"));
        let v2 = Value::Str(Sds::from_str("two"));

        store.set(k.clone(), v1.clone()).unwrap();
        store.set(k.clone(), v2.clone()).unwrap();

        let got = store.get(k.clone()).unwrap();
        assert_eq!(got, Some(v2));
    }

    /// Проверяет, что ключ можно удалить, и после этого он недоступен для получения.
    #[test]
    fn test_delete() {
        let mut store = InMemoryStore::new();
        let k = key("key_to_delete");
        let v = Value::Str(Sds::from_str("some_value"));

        store.set(k.clone(), v).unwrap();
        store.del(k.clone()).unwrap();

        let got = store.get(k.clone()).unwrap();
        assert_eq!(got, None);
    }

    /// Проверяет, что получение значения по несуществующему ключу возвращает None.
    #[test]
    fn test_get_nonexistent_key() {
        let mut store = InMemoryStore::new();
        let got = store.get(key("missing")).unwrap();
        assert_eq!(got, None);
    }

    /// Проверяет, что удаление несуществующего ключа не приводит к ошибке.
    #[test]
    fn test_delete_nonexistent_key() {
        let store = InMemoryStore::new();
        // Удаление несуществующего ключа не должно вызывать ошибку.
        assert!(store.del(key("nope")).is_ok());
    }

    /// Тестирует функциональность массовой установки и массового получения значений.
    /// Проверяет корректность получения существующих и несуществующих ключей.
    #[test]
    fn test_mset_and_mget() {
        let mut store = InMemoryStore::new();
        let entries = vec![
            (key("key1"), Value::Int(1)),
            (key("key2"), Value::Int(2)),
            (key("key3"), Value::Int(3)),
        ];
        store.mset(entries.clone()).unwrap();

        let keys = vec![key("key1"), key("key2"), key("key3"), key("missing")];
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

    /// Проверяет, что переименование существующего ключа происходит корректно.
    #[test]
    fn test_rename() {
        let mut store = InMemoryStore::new();
        store.set(key("old"), Value::Int(123)).unwrap();

        store.rename(key("old"), key("new")).unwrap();
        assert!(store.get(key("old")).unwrap().is_none());
        assert_eq!(store.get(key("new")).unwrap(), Some(Value::Int(123)));
    }

    /// Проверяет, что попытка переименования несуществующего ключа приводит к ошибке
    /// с кодом KeyNotFound.
    #[test]
    fn test_rename_nonexistent_key() {
        let mut store = InMemoryStore::new();
        let result = store.rename(key("does_not_exist"), key("whatever"));
        assert!(matches!(result, Err(StoreError::KeyNotFound)));
    }

    /// Тестирует работу метода renamenx: переименование происходит только
    /// если целевой ключ отсутствует.
    #[test]
    fn test_renamenx_success() {
        let mut store = InMemoryStore::new();
        store
            .set(key("old"), Value::Str(Sds::from_str("val")))
            .unwrap();

        let ok = store.renamenx(key("old"), key("new")).unwrap();
        assert!(ok);
        assert!(store.get(key("old")).unwrap().is_none());
        assert_eq!(
            store.get(key("new")).unwrap(),
            Some(Value::Str(Sds::from_str("val")))
        );
    }

    /// Проверяет, что renamenx не выполняется, если целевой ключ уже существует.
    #[test]
    fn test_renamenx_existing_target() {
        let mut store = InMemoryStore::new();
        store.set(key("old"), Value::Int(1)).unwrap();
        store.set(key("new"), Value::Int(2)).unwrap();

        let ok = store.renamenx(key("old"), key("new")).unwrap();
        assert!(!ok); // Ожидается false, так как целевой ключ уже существует.
        assert_eq!(store.get(key("old")).unwrap(), Some(Value::Int(1)));
        assert_eq!(store.get(key("new")).unwrap(), Some(Value::Int(2)));
    }

    /// Проверяет, что метод flushdb очищает хранилище от всех ключей и значений.
    #[test]
    fn test_flushdb() {
        let mut store = InMemoryStore::new();
        store.set(key("one"), Value::Int(1)).unwrap();
        store.set(key("two"), Value::Int(2)).unwrap();

        store.flushdb().unwrap();

        assert!(store.get(key("one")).unwrap().is_none());
        assert!(store.get(key("two")).unwrap().is_none());
    }
}
