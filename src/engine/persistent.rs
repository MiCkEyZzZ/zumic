use std::{collections::HashMap, path::Path, sync::Mutex};

use super::{
    aof::{AofOp, SyncPolicy},
    AofLog, Storage,
};
use crate::{Sds, StoreError, StoreResult, Value};

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
