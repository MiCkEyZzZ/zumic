//! Команды для работы с множествами (Set) в Zumic.
//!
//! Реализует команды SADD, SREM, SISMEMBER, SMEMBERS, SCARD для управления
//! элементами множеств по ключу. Каждая команда реализует трейт
//! [`CommandExecute`].

use crate::{CommandExecute, QuickList, Sds, StorageEngine, StoreError, Value};

/// Команда SADD — добавляет элемент во множество.
///
/// Формат: `SADD key member`
///
/// # Поля
/// * `key` — ключ множества.
/// * `member` — добавляемый элемент.
///
/// # Возвращает
/// 1, если элемент был добавлен, 0 — если уже существовал.
#[derive(Debug)]
pub struct SAddCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for SAddCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        let added = store.sadd(&key, std::slice::from_ref(&member))?;
        Ok(Value::Int(added as i64))
    }
}

/// Команда SREM — удаляет элемент из множества.
///
/// Формат: `SREM key member`
///
/// # Поля
/// * `key` — ключ множества.
/// * `member` — удаляемый элемент.
///
/// # Возвращает
/// 1, если элемент был удалён, 0 — если не найден.
#[derive(Debug)]
pub struct SRemCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for SRemCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        let removed = store.srem(&key, std::slice::from_ref(&member))?;
        Ok(Value::Int(removed as i64))
    }
}

/// Команда SISMEMBER — проверяет наличие элемента во множестве.
///
/// Формат: `SISMEMBER key member`
///
/// # Поля
/// * `key` — ключ множества.
/// * `member` — проверяемый элемент.
///
/// # Возвращает
/// 1, если элемент найден, 0 — если не найден.
#[derive(Debug)]
pub struct SIsMemberCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for SIsMemberCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        let exists = store.sismember(&key, &member)?;
        Ok(Value::Int(exists as i64))
    }
}

/// Команда SMEMBERS — возвращает все элементы множества.
///
/// Формат: `SMEMBERS key`
///
/// # Поля
/// * `key` — ключ множества.
///
/// # Возвращает
/// Список всех элементов множества или `Null`, если множество не существует.
#[derive(Debug)]
pub struct SMembersCommand {
    pub key: String,
}

impl CommandExecute for SMembersCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Set(_)) => {
                let members: Vec<Sds> = store.smembers(&key)?;
                let list = QuickList::from_iter(members, 64);
                Ok(Value::List(list))
            }
            Some(Value::Null) | None => Ok(Value::Null),
            Some(_) => Err(StoreError::WrongType("SMEMBERS on non-set key".into())),
        }
    }
}

/// Команда SCARD — возвращает количество элементов во множестве.
///
/// Формат: `SCARD key`
///
/// # Поля
/// * `key` — ключ множества.
///
/// # Возвращает
/// Количество элементов во множестве.
#[derive(Debug)]
pub struct SCardCommand {
    pub key: String,
}

impl CommandExecute for SCardCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let count = store.scard(&key)?;
        Ok(Value::Int(count as i64))
    }
}

/// Команда SRANDMEMBER — возвращает случайный(е) элемент(ы) множества.
///
/// Формат: `SRANDMEMBER key count`
/// Если count == 1 — вернём массив с одним элементом (упрощённо).
/// Если ключ отсутствует — возвращаем Null.
#[derive(Debug)]
pub struct SRandMemberCommand {
    pub key: String,
    pub count: isize,
}

impl CommandExecute for SRandMemberCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            None | Some(Value::Null) => return Ok(Value::Null),
            _ => {}
        }

        let members: Vec<Sds> = store.srandmember(&key, self.count)?;
        if members.is_empty() {
            Ok(Value::Null)
        } else {
            let list = QuickList::from_iter(members, 64);
            Ok(Value::List(list))
        }
    }
}

/// Команда SPOP — удаляет и возвращает случайный(е) элемент(ы) множества.
///
/// Формат: `SPOP key count`
/// Если ключ отсутствует — возвращаем Null.
#[derive(Debug)]
pub struct SPopCommand {
    pub key: String,
    pub count: isize,
}

impl CommandExecute for SPopCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            None | Some(Value::Null) => return Ok(Value::Null),
            _ => {}
        }

        let removed: Vec<Sds> = store.spop(&key, self.count)?;
        if removed.is_empty() {
            Ok(Value::Null)
        } else {
            let list = QuickList::from_iter(removed, 64);
            Ok(Value::List(list))
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

    /// Тест, который проверяет, что SAddCommand добавляет новый элемент в
    /// множество. Первоначальная вставка должна вернуть 1 (элемент
    /// добавлен), а вторая вставка того же элемента должна вернуть 0.
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

    /// Тест, который проверяет, что SCardCommand возвращает правильную
    /// кардинальность множества.
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

    /// Тест, который проверяет, что SCardCommand возвращает ноль, если ключ не
    /// существует.
    #[test]
    fn test_scard_nonexistent_key() {
        let mut store = create_store();

        let scard = SCardCommand {
            key: "empty".to_string(),
        };
        let result = scard.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    /// Тест, который проверяет, что SRemCommand успешно удаляет существующий
    /// элемент из множества. Он должен вернуть 1 при удалении элемента и 0
    /// при попытке удалить тот же элемент снова.
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

    /// Тест, который проверяет, что SIsMemberCommand корректно определяет
    /// наличие значения в множестве.
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

    /// Тест, который проверяет, что SMembersCommand возвращает все элементы
    /// множества. Он должен вернуть QuickList, содержащий все элементы как
    /// ArcBytes.
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

    /// Тест, который проверяет, что SMembersCommand возвращает Null, если ключ
    /// не существует.
    #[test]
    fn test_smembers_nonexistent_key() {
        let mut store = create_store();

        let smembers = SMembersCommand {
            key: "missing".to_string(),
        };
        let result = smembers.execute(&mut store).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_srandmember_single_and_multiple() {
        let mut store = create_store();

        // заполняем множество
        let sadd = SAddCommand {
            // подставьте путь если у вас иначе
            key: "myset".to_string(),
            member: "a".to_string(),
        };
        sadd.execute(&mut store).unwrap();
        let sadd = SAddCommand {
            key: "myset".to_string(),
            member: "b".to_string(),
        };
        sadd.execute(&mut store).unwrap();
        let sadd = SAddCommand {
            key: "myset".to_string(),
            member: "c".to_string(),
        };
        sadd.execute(&mut store).unwrap();

        let cmd = SRandMemberCommand {
            key: "myset".into(),
            count: 1,
        };
        let res = cmd.execute(&mut store).unwrap();
        match res {
            Value::List(l) => assert_eq!(l.len(), 1),
            _ => panic!("Expected list"),
        }

        let cmd = SRandMemberCommand {
            key: "myset".into(),
            count: 2,
        };
        let res = cmd.execute(&mut store).unwrap();
        match res {
            Value::List(l) => assert!((1..=2).contains(&l.len())),
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_spop_removes() {
        let mut store = create_store();

        // подготовка
        let sadd = SAddCommand {
            key: "myset".to_string(),
            member: "a".to_string(),
        };
        sadd.execute(&mut store).unwrap();
        let sadd = SAddCommand {
            key: "myset".to_string(),
            member: "b".to_string(),
        };
        sadd.execute(&mut store).unwrap();

        // SPOP 1
        let pop1 = SPopCommand {
            key: "myset".into(),
            count: 1,
        };
        let res1 = pop1.execute(&mut store).unwrap();
        match res1 {
            Value::List(l) => assert_eq!(l.len(), 1),
            _ => panic!("Expected list"),
        }

        // SPOP 10 (больше, чем осталось) — удалит оставшиеся
        let pop_all = SPopCommand {
            key: "myset".into(),
            count: 10,
        };
        let res_all = pop_all.execute(&mut store).unwrap();
        match res_all {
            Value::List(l) => {
                // один элемент должен был остаться и быть возвращён
                assert!(l.len() <= 1);
            }
            Value::Null => {
                // если предыдущее споп удалил всё, допустимо получить Null
            }
            _ => panic!("Expected list or null"),
        }
    }
}
