use std::sync::Arc;

use dashmap::DashMap;

use crate::{Sds, Storage, StoreError, StoreResult, Value};

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
}

impl InMemoryStore {
    /// Создаёт новый, пустой `InMemoryStore`.
    pub fn new() -> Self {
        Self {
            #[allow(clippy::arc_with_non_send_sync)]
            data: Arc::new(DashMap::new()),
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
    fn set(&self, key: &Sds, value: Value) -> StoreResult<()> {
        self.data.insert(key.clone(), value);
        Ok(())
    }

    /// Получает значение по ключу.
    ///
    /// Возвращает `Ok(Some(value))`, если ключ существует, иначе `Ok(None)`.
    fn get(&self, key: &Sds) -> StoreResult<Option<Value>> {
        Ok(self.data.get(key).map(|entry| entry.value().clone()))
    }

    /// Удаляет ключ. Возвращает `Ok(true)`, если ключ действительно был удалён,
    /// и `Ok(false)`, если ключ не существовал.
    fn del(&self, key: &Sds) -> StoreResult<bool> {
        Ok(self.data.remove(key).is_some())
    }

    /// Массовая установка значений.
    ///
    /// Устанавливает все переданные пары ключ-значение. Ключи клонируются.
    fn mset(&self, entries: Vec<(&Sds, Value)>) -> StoreResult<()> {
        for (key, value) in entries {
            self.data.insert(key.clone(), value);
        }
        Ok(())
    }

    /// Массовое получение значений по ключам.
    ///
    /// Возвращает вектор опциональных значений, соответствующих переданным ключам.
    fn mget(&self, keys: &[&Sds]) -> StoreResult<Vec<Option<Value>>> {
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
    fn rename(&self, from: &Sds, to: &Sds) -> StoreResult<()> {
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
    fn renamenx(&self, from: &Sds, to: &Sds) -> StoreResult<bool> {
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
}
