use std::{collections::HashMap, path::Path, sync::Mutex};

use super::{AofLog, Storage};
use crate::{Sds, StoreError, StoreResult, Value};

pub struct InPersistentStore {
    index: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    aof: Mutex<AofLog>,
}

impl InPersistentStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StoreError> {
        let mut aof = AofLog::open(path)?;
        let mut index = HashMap::new();

        // восстановление in-memory из AOF
        aof.replay(|op, key, val| match op {
            1 => {
                if let Some(value) = val {
                    index.insert(key, value);
                }
            }
            2 => {
                index.remove(&key);
            }
            _ => {}
        })?;

        Ok(Self {
            index: Mutex::new(index),
            aof: Mutex::new(aof),
        })
    }
}

impl Storage for InPersistentStore {
    fn set(&self, key: &Sds, value: Value) -> StoreResult<()> {
        let key_b = key.as_bytes();
        let val_b = value.to_bytes();
        self.aof.lock().unwrap().append_set(key_b, &val_b)?;
        self.index.lock().unwrap().insert(key_b.to_vec(), val_b);
        Ok(())
    }

    fn get(&self, key: &Sds) -> StoreResult<Option<Value>> {
        let key_b = key.as_bytes();
        let map = self.index.lock().unwrap();
        match map.get(key_b) {
            Some(val) => Ok(Some(Value::from_bytes(val)?)),
            None => Ok(None),
        }
    }

    fn del(&self, key: &Sds) -> StoreResult<i64> {
        let key_b = key.as_bytes();
        let mut map = self.index.lock().unwrap();
        if map.remove(key_b).is_some() {
            self.aof.lock().unwrap().append_del(key_b)?;
            Ok(1)
        } else {
            Ok(0)
        }
    }

    fn mset(&self, entries: Vec<(&Sds, Value)>) -> StoreResult<()> {
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

    fn mget(&self, keys: &[&Sds]) -> StoreResult<Vec<Option<Value>>> {
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

    fn rename(&self, from: &Sds, to: &Sds) -> StoreResult<()> {
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

    fn renamenx(&self, from: &Sds, to: &Sds) -> StoreResult<bool> {
        let mut map = self.index.lock().unwrap();
        let from_b = from.as_bytes();
        let to_b = to.as_bytes();

        if !map.contains_key(from_b) {
            return Err(StoreError::KeyNotFound);
        }

        if map.contains_key(to_b) {
            return Ok(false);
        }

        if let Some(val) = map.remove(from_b) {
            self.aof.lock().unwrap().append_del(from_b)?;
            self.aof.lock().unwrap().append_set(to_b, &val)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn flushdb(&self) -> StoreResult<()> {
        let mut map = self.index.lock().unwrap();
        map.clear();
        // можно реализовать AOF truncate или просто удалить файл - пока опускаем.
        Ok(())
    }
}
