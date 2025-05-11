// Copyright 2025 Zumic

use std::collections::HashSet;

use crate::{CommandExecute, QuickList, Sds, StorageEngine, StoreError, Value};

#[derive(Debug)]
pub struct SAddCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for SAddCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        match store.get(&key)? {
            Some(Value::Set(mut set)) => {
                let inserted = set.insert(member.clone());
                store.set(&key, Value::Set(set))?;
                Ok(Value::Int(inserted as i64))
            }
            Some(Value::Null) | None => {
                let mut set = HashSet::new();
                set.insert(member);
                store.set(&key, Value::Set(set))?;
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
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        if let Some(Value::Set(mut set)) = store.get(&key)? {
            let removed = set.remove(&member);
            store.set(&key, Value::Set(set))?;
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
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        if let Some(Value::Set(set)) = store.get(&key)? {
            Ok(Value::Int(set.contains(&member) as i64))
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
        let key = Sds::from_str(&self.key);

        if let Some(Value::Set(set)) = store.get(&key)? {
            let list = QuickList::from_iter(set.iter().cloned(), 64);
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
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Set(set)) => Ok(Value::Int(set.len() as i64)),
            Some(Value::Null) | None => Ok(Value::Int(0)),
            _ => Err(StoreError::InvalidType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    // Вспомогательная функция для создания нового хранилища в памяти.
    fn create_store() -> StorageEngine {
        StorageEngine::InMemory(InMemoryStore::new())
    }

    /// Тест, который проверяет, что SAddCommand добавляет новый элемент в множество.
    /// Первоначальная вставка должна вернуть 1 (элемент добавлен), а
    /// вторая вставка того же элемента должна вернуть 0.
    #[test]
    fn test_sadd_command() {
        let mut store = create_store();

        let sadd = SAddCommand {
            key: "myset".to_string(),
            member: "one".to_string(),
        };

        // Первая вставка добавляет элемент.
        let result = sadd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(1));

        // Вторая вставка не добавляет дубликат.
        let result = sadd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    /// Тест, который проверяет, что SCardCommand возвращает правильную кардинальность множества.
    #[test]
    fn test_scard_command() {
        let mut store = create_store();

        let sadd1 = SAddCommand {
            key: "numbers".to_string(),
            member: "one".to_string(),
        };
        let sadd2 = SAddCommand {
            key: "numbers".to_string(),
            member: "two".to_string(),
        };

        sadd1.execute(&mut store).unwrap();
        sadd2.execute(&mut store).unwrap();

        let scard = SCardCommand {
            key: "numbers".to_string(),
        };
        let result = scard.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    /// Тест, который проверяет, что SCardCommand возвращает ноль, если ключ не существует.
    #[test]
    fn test_scard_nonexistent_key() {
        let mut store = create_store();

        let scard = SCardCommand {
            key: "empty".to_string(),
        };
        let result = scard.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    /// Тест, который проверяет, что SRemCommand успешно удаляет существующий элемент из множества.
    /// Он должен вернуть 1 при удалении элемента и 0 при попытке удалить тот же элемент снова.
    #[test]
    fn test_srem_command() {
        let mut store = create_store();

        let sadd = SAddCommand {
            key: "myset".to_string(),
            member: "one".to_string(),
        };
        sadd.execute(&mut store).unwrap();

        let srem = SRemCommand {
            key: "myset".to_string(),
            member: "one".to_string(),
        };
        let result = srem.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(1));

        let srem_again = SRemCommand {
            key: "myset".to_string(),
            member: "one".to_string(),
        };
        let result = srem_again.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    /// Тест, который проверяет, что SIsMemberCommand корректно определяет наличие значения
    /// в множестве.
    /// Он должен вернуть 1, если элемент существует, и 0, если не существует.
    #[test]
    fn test_sismember_command() {
        let mut store = create_store();

        let sadd = SAddCommand {
            key: "myset".to_string(),
            member: "alpha".to_string(),
        };
        sadd.execute(&mut store).unwrap();

        let sismember = SIsMemberCommand {
            key: "myset".to_string(),
            member: "alpha".to_string(),
        };
        let result = sismember.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(1));

        let not_member = SIsMemberCommand {
            key: "myset".to_string(),
            member: "beta".to_string(),
        };
        let result = not_member.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    /// Тест, который проверяет, что SMembersCommand возвращает все элементы множества.
    /// Он должен вернуть QuickList, содержащий все элементы как ArcBytes.
    #[test]
    fn test_smembers_command() {
        let mut store = create_store();

        let sadd1 = SAddCommand {
            key: "tags".to_string(),
            member: "a".to_string(),
        };
        let sadd2 = SAddCommand {
            key: "tags".to_string(),
            member: "b".to_string(),
        };
        sadd1.execute(&mut store).unwrap();
        sadd2.execute(&mut store).unwrap();

        let smembers = SMembersCommand {
            key: "tags".to_string(),
        };
        let result = smembers.execute(&mut store).unwrap();
        match result {
            Value::List(list) => {
                let mut values = list.iter().map(|v| v.to_string()).collect::<Vec<_>>();
                values.sort();
                assert_eq!(values, vec!["a", "b"]);
            }
            _ => panic!("Expected Value::List"),
        }
    }

    /// Тест, который проверяет, что SMembersCommand возвращает Null, если ключ не существует.
    #[test]
    fn test_smembers_nonexistent_key() {
        let mut store = create_store();

        let smembers = SMembersCommand {
            key: "missing".to_string(),
        };
        let result = smembers.execute(&mut store).unwrap();
        assert_eq!(result, Value::Null);
    }
}
