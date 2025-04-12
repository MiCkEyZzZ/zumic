use std::collections::HashSet;

use crate::{
    database::{ArcBytes, QuickList, Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct SAddCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for SAddCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        match store.get(key.clone())? {
            Some(Value::Set(mut set)) => {
                let inserted = set.insert(self.member.clone());
                store.set(key.clone(), Value::Set(set))?;
                Ok(Value::Int(inserted as i64))
            }
            Some(Value::Null) | None => {
                let mut set = HashSet::new();
                set.insert(self.member.clone());
                store.set(key, Value::Set(set))?;
                Ok(Value::Int(1))
            }
            _ => Err(StoreError::InvalidType),
        }
    }
}

#[derive(Debug)]
pub struct SRemCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for SRemCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        if let Some(Value::Set(mut set)) = store.get(key.clone())? {
            let removed = set.remove(&self.member);
            store.set(key, Value::Set(set))?;
            Ok(Value::Int(removed as i64))
        } else {
            Ok(Value::Int(0))
        }
    }
}

#[derive(Debug)]
pub struct SIsMemberCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for SIsMemberCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        if let Some(Value::Set(set)) = store.get(key)? {
            Ok(Value::Int(set.contains(&self.member) as i64))
        } else {
            Ok(Value::Int(0))
        }
    }
}

#[derive(Debug)]
pub struct SMembersCommand {
    pub key: String,
}

impl CommandExecute for SMembersCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        if let Some(Value::Set(set)) = store.get(key)? {
            let list = QuickList::from_iter(set.iter().map(|s| ArcBytes::from(s.as_str())), 64);
            Ok(Value::List(list))
        } else {
            Ok(Value::Null)
        }
    }
}

#[derive(Debug)]
pub struct SCardCommand {
    pub key: String,
}

impl CommandExecute for SCardCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        match store.get(key)? {
            Some(Value::Set(set)) => Ok(Value::Int(set.len() as i64)),
            Some(Value::Null) | None => Ok(Value::Int(0)),
            _ => Err(StoreError::InvalidType),
        }
    }
}
