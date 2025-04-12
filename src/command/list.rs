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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::memory::InMemoryStore;

    fn create_store() -> StorageEngine {
        StorageEngine::InMemory(InMemoryStore::new())
    }

    #[test]
    fn test_lpush_and_llen_and_lpop() {
        let mut store = create_store();

        let cmd = LPushCommand {
            key: "l".into(),
            value: "one".into(),
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(1));

        let cmd2 = LPushCommand {
            key: "l".into(),
            value: "two".into(),
        };
        assert_eq!(cmd2.execute(&mut store).unwrap(), Value::Int(2));

        let llen = LLenCommand { key: "l".into() };
        assert_eq!(llen.execute(&mut store).unwrap(), Value::Int(2));

        let lpop = LPopCommand { key: "l".into() };
        assert_eq!(
            lpop.execute(&mut store).unwrap(),
            Value::Str(ArcBytes::from_str("two"))
        );
        assert_eq!(
            LLenCommand { key: "l".into() }.execute(&mut store).unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn test_rpush_and_rpop() {
        let mut store = create_store();

        let cmd = RPushCommand {
            key: "r".into(),
            value: "a".into(),
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(1));
        let cmd2 = RPushCommand {
            key: "r".into(),
            value: "b".into(),
        };
        assert_eq!(cmd2.execute(&mut store).unwrap(), Value::Int(2));

        let rpop = RPopCommand { key: "r".into() };
        assert_eq!(
            rpop.execute(&mut store).unwrap(),
            Value::Str(ArcBytes::from_str("b"))
        );
        assert_eq!(
            LLenCommand { key: "r".into() }.execute(&mut store).unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn test_lrange_positive_and_negative() {
        let mut store = create_store();
        for v in &["x", "y", "z"] {
            RPushCommand {
                key: "lr".into(),
                value: v.to_string(),
            }
            .execute(&mut store)
            .unwrap();
        }

        // полный диапазон
        let range = LRangeCommand {
            key: "lr".into(),
            start: 0,
            stop: -1,
        };
        let list = match range.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list,
            vec![
                ArcBytes::from_str("x"),
                ArcBytes::from_str("y"),
                ArcBytes::from_str("z"),
            ]
        );

        // частичный
        let range2 = LRangeCommand {
            key: "lr".into(),
            start: 1,
            stop: 1,
        };
        let list2 = match range2.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(list2, vec![ArcBytes::from_str("y")]);
    }

    #[test]
    fn test_len_and_pop_nonexistent_and_type_error() {
        let mut store = create_store();

        // пустой список
        assert_eq!(
            LLenCommand { key: "no".into() }
                .execute(&mut store)
                .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            LPopCommand { key: "no".into() }
                .execute(&mut store)
                .unwrap(),
            Value::Null
        );

        // несоответствующий тип
        store.set(ArcBytes::from_str("k"), Value::Int(5)).unwrap();
        assert!(matches!(
            LPushCommand {
                key: "k".into(),
                value: "v".into()
            }
            .execute(&mut store),
            Err(StoreError::InvalidType)
        ));
    }
}
