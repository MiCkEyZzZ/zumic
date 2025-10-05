//! Команды для работы со списками (List) в Zumic.
//!
//! Реализует команды LPUSH, RPUSH, LPOP, RPOP, LLEN, LRANGE для управления
//! элементами списков по ключу. Каждая команда реализует трейт
//! [`CommandExecute`].

use crate::{CommandExecute, QuickList, Sds, StorageEngine, StoreError, Value};

/// Команда LPUSH — добавляет элемент в начало списка.
///
/// Формат: `LPUSH key value`
///
/// # Поля
/// * `key` — ключ списка.
/// * `value` — добавляемое значение.
///
/// # Возвращает
/// Новая длина списка после вставки.
#[derive(Debug)]
pub struct LPushCommand {
    pub key: String,
    pub value: String,
}

impl CommandExecute for LPushCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let element = Sds::from_str(&self.value);

        let mut list = match store.get(&key)? {
            Some(Value::List(list)) => list,
            Some(_) => return Err(StoreError::InvalidType),
            None => QuickList::new(64),
        };

        list.push_front(element);
        let len = list.len() as i64;
        store.set(&key, Value::List(list))?;
        Ok(Value::Int(len))
    }
}

/// Команда RPUSH — добавляет элемент в конец списка.
///
/// Формат: `RPUSH key value`
///
/// # Поля
/// * `key` — ключ списка.
/// * `value` — добавляемое значение.
///
/// # Возвращает
/// Новая длина списка после вставки.
#[derive(Debug)]
pub struct RPushCommand {
    pub key: String,
    pub value: String,
}

impl CommandExecute for RPushCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let element = Sds::from_str(&self.value);

        let mut list = match store.get(&key)? {
            Some(Value::List(list)) => list,
            Some(_) => return Err(StoreError::InvalidType),
            None => QuickList::new(64),
        };

        list.push_back(element);
        let len = list.len() as i64;
        store.set(&key, Value::List(list))?;
        Ok(Value::Int(len))
    }
}

/// Команда LPOP — удаляет и возвращает первый элемент списка.
///
/// Формат: `LPOP key`
///
/// # Поля
/// * `key` — ключ списка.
///
/// # Возвращает
/// Значение первого элемента или `Null`, если список пуст или не существует.
#[derive(Debug)]
pub struct LPopCommand {
    pub key: String,
}

impl CommandExecute for LPopCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::List(mut list)) => {
                if let Some(elem) = list.pop_front() {
                    store.set(&key, Value::List(list))?;
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

/// Команда RPOP — удаляет и возвращает последний элемент списка.
///
/// Формат: `RPOP key`
///
/// # Поля
/// * `key` — ключ списка.
///
/// # Возвращает
/// Значение последнего элемента или `Null`, если список пуст или не существует.
#[derive(Debug)]
pub struct RPopCommand {
    pub key: String,
}

impl CommandExecute for RPopCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::List(mut list)) => {
                if let Some(elem) = list.pop_back() {
                    store.set(&key, Value::List(list))?;
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

/// Команда LLEN — возвращает длину списка.
///
/// Формат: `LLEN key`
///
/// # Поля
/// * `key` — ключ списка.
///
/// # Возвращает
/// Длина списка (количество элементов).
#[derive(Debug)]
pub struct LLenCommand {
    pub key: String,
}

impl CommandExecute for LLenCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        match store.get(&key)? {
            Some(Value::List(list)) => Ok(Value::Int(list.len() as i64)),
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }
}

/// Команда LRANGE — возвращает диапазон элементов списка.
///
/// Формат: `LRANGE key start stop`
///
/// # Поля
/// * `key` — ключ списка.
/// * `start` — начальный индекс.
/// * `stop` — конечный индекс.
///
/// # Возвращает
/// Список элементов в заданном диапазоне или `Null`, если список не существует.
#[derive(Debug)]
pub struct LRangeCommand {
    pub key: String,
    pub start: i64,
    pub stop: i64,
}

impl CommandExecute for LRangeCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        match store.get(&key)? {
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
                let ql = QuickList::from_iter(vec, 64);
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
    use crate::InMemoryStore;

    // Вспомогательная функция для создания нового хранилища в памяти.
    fn create_store() -> StorageEngine {
        StorageEngine::Memory(InMemoryStore::new())
    }

    /// Тест, что LPushCommand правильно добавляет элемент в начало списка,
    /// обновляет длину списка, а LPopCommand удаляет правильный элемент.
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
            Value::Str(Sds::from_str("two"))
        );
        assert_eq!(
            LLenCommand { key: "l".into() }.execute(&mut store).unwrap(),
            Value::Int(1)
        );
    }

    /// Тест, что RPushCommand правильно добавляет элементы в конец списка,
    /// а RPopCommand правильно удаляет последний элемент.
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
            Value::Str(Sds::from_str("b"))
        );
        assert_eq!(
            LLenCommand { key: "r".into() }.execute(&mut store).unwrap(),
            Value::Int(1)
        );
    }

    /// Тест, что LRangeCommand корректно извлекает элементы при использовании
    /// как положительных, так и отрицательных индексов.
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

        // Извлечение полного диапазона с start=0 и stop=-1.
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
            vec![Sds::from_str("x"), Sds::from_str("y"), Sds::from_str("z"),]
        );

        // Извлечение только элемента с индексом 1.
        let range2 = LRangeCommand {
            key: "lr".into(),
            start: 1,
            stop: 1,
        };
        let list2 = match range2.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(list2, vec![Sds::from_str("y")]);
    }

    /// Тест, что LLenCommand возвращает 0 и LPopCommand возвращает Null, когда
    /// список не существует, и что возникает ошибка типа, если ключ существует,
    /// но его тип не список.
    #[test]
    fn test_len_and_pop_nonexistent_and_type_error() {
        let mut store = create_store();

        // Для несуществующего ключа, LLen должен вернуть 0, а LPop вернуть Null.
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

        // Если ключ существует, но это не список, то LPush должен вернуть ошибку
        // InvalidType.
        store.set(&Sds::from_str("k"), Value::Int(5)).unwrap();
        assert!(matches!(
            LPushCommand {
                key: "k".into(),
                value: "v".into(),
            }
            .execute(&mut store),
            Err(StoreError::InvalidType)
        ));
    }
}
