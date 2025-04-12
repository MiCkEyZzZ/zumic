use crate::{
    database::{ArcBytes, QuickList, Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct LPushCommand {
    pub key: String,
    pub value: String,
}

impl CommandExecute for LPushCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let element = ArcBytes::from_str(&self.value);

        let mut list = match store.get(key.clone())? {
            Some(Value::List(list)) => list,
            Some(_) => return Err(StoreError::InvalidType),
            None => QuickList::new(64),
        };

        list.push_front(element);
        let len = list.len() as i64;
        store.set(key, Value::List(list))?;
        Ok(Value::Int(len))
    }
}

#[derive(Debug)]
pub struct RPushCommand {
    pub key: String,
    pub value: String,
}

impl CommandExecute for RPushCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let element = ArcBytes::from_str(&self.value);

        let mut list = match store.get(key.clone())? {
            Some(Value::List(list)) => list,
            Some(_) => return Err(StoreError::InvalidType),
            None => QuickList::new(64),
        };

        list.push_back(element);
        let len = list.len() as i64;
        store.set(key, Value::List(list))?;
        Ok(Value::Int(len))
    }
}

#[derive(Debug)]
pub struct LPopCommand {
    pub key: String,
}

impl CommandExecute for LPopCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        match store.get(key.clone())? {
            Some(Value::List(mut list)) => {
                if let Some(elem) = list.pop_front() {
                    store.set(key, Value::List(list))?;
                    Ok(Value::Str(elem))
                } else {
                    Ok(Value::Null)
                }
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Null),
        }
    }
}

#[derive(Debug)]
pub struct RPopCommand {
    pub key: String,
}

impl CommandExecute for RPopCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        match store.get(key.clone())? {
            Some(Value::List(mut list)) => {
                if let Some(elem) = list.pop_back() {
                    store.set(key, Value::List(list))?;
                    Ok(Value::Str(elem))
                } else {
                    Ok(Value::Null)
                }
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Null),
        }
    }
}

#[derive(Debug)]
pub struct LLenCommand {
    pub key: String,
}

impl CommandExecute for LLenCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        match store.get(key)? {
            Some(Value::List(list)) => Ok(Value::Int(list.len() as i64)),
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }
}

#[derive(Debug)]
pub struct LRangeCommand {
    pub key: String,
    pub start: i64,
    pub stop: i64,
}

impl CommandExecute for LRangeCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        match store.get(key)? {
            Some(Value::List(list)) => {
                let len = list.len() as i64;
                let s = if self.start < 0 {
                    (len + self.start).max(0)
                } else {
                    self.start.min(len)
                } as usize;
                let e = if self.stop < 0 {
                    (len + self.stop).max(0)
                } else {
                    self.stop.min(len - 1)
                } as usize;
                let mut vec = Vec::new();
                for idx in s..=e.min(list.len().saturating_sub(1)) {
                    if let Some(elem) = list.get(idx) {
                        vec.push(elem.clone());
                    }
                }
                let ql = QuickList::from_iter(vec.into_iter(), 64);
                Ok(Value::List(ql))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Null),
        }
    }
}
