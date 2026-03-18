use crate::{CommandExecute, Sds, StorageEngine, StoreError, Value};

#[inline]
fn value_to_i64(value: Value) -> Result<i64, StoreError> {
    match value {
        Value::Int(n) => Ok(n),
        // Redis-совместимость: разрешаем строковые числа.
        Value::Str(ref sds) => sds.to_i64().map_err(|_| StoreError::InvalidType),
        _ => Err(StoreError::InvalidType),
    }
}

#[cold]
#[inline(never)]
fn overflow_err() -> StoreError {
    // TODO(#SDS-14): позже заменим на StoreError::Overflow когда сделаю реализацию
    // в zumic-error/src/types/storage.rs
    StoreError::InvalidType
}

/// Команда INCR — увеличивает целочисленное значение по ключу на 1.
#[derive(Debug)]
pub struct IncrCommand {
    pub key: String,
}

impl CommandExecute for IncrCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let current = match store.get(&key)? {
            Some(v) => value_to_i64(v)?,
            None => 0,
        };
        let new_val = current.checked_add(1).ok_or_else(overflow_err)?;

        store.set(&key, Value::Int(new_val))?;

        Ok(Value::Int(new_val))
    }

    fn command_name(&self) -> &'static str {
        "INCR"
    }
}

/// Команда INCRBY — увеличивает целочисленное значение по ключу на заданное
/// число.
#[derive(Debug)]
pub struct IncrByCommand {
    pub key: String,
    pub increment: i64,
}

impl CommandExecute for IncrByCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let current = match store.get(&key)? {
            Some(v) => value_to_i64(v)?,
            None => 0,
        };
        let new_val = current
            .checked_add(self.increment)
            .ok_or_else(overflow_err)?;

        store.set(&key, Value::Int(new_val))?;

        Ok(Value::Int(new_val))
    }

    fn command_name(&self) -> &'static str {
        "INCRBY"
    }
}

/// Команда DECR — уменьшает целочисленное значение по ключу на 1.
#[derive(Debug)]
pub struct DecrCommand {
    pub key: String,
}

impl CommandExecute for DecrCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let current = match store.get(&key)? {
            Some(v) => value_to_i64(v)?,
            None => 0,
        };
        let new_val = current.checked_sub(1).ok_or_else(overflow_err)?;

        store.set(&key, Value::Int(new_val))?;

        Ok(Value::Int(new_val))
    }

    fn command_name(&self) -> &'static str {
        "DECR"
    }
}

/// Команда DECRBY — уменьшает целочисленное значение по ключу на заданное
/// число.
#[derive(Debug)]
pub struct DecrByCommand {
    pub key: String,
    pub decrement: i64,
}

impl CommandExecute for DecrByCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let current = match store.get(&key)? {
            Some(v) => value_to_i64(v)?,
            None => 0,
        };
        let new_val = current
            .checked_sub(self.decrement)
            .ok_or_else(overflow_err)?;

        store.set(&key, Value::Int(new_val))?;

        Ok(Value::Int(new_val))
    }

    fn command_name(&self) -> &'static str {
        "DECRBY"
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryStore, Sds};

    fn mem_store() -> StorageEngine {
        StorageEngine::Memory(InMemoryStore::new())
    }

    #[test]
    fn test_incr_missing_key_starts_at_one() {
        let mut store = mem_store();

        assert_eq!(
            IncrCommand { key: "c".into() }.execute(&mut store).unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn test_incr_existing_int() {
        let mut store = mem_store();

        store.set(&Sds::from_str("c"), Value::Int(41)).unwrap();

        assert_eq!(
            IncrCommand { key: "c".into() }.execute(&mut store).unwrap(),
            Value::Int(42)
        );
    }

    #[test]
    fn test_incr_string_value_redis_compat() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Str(Sds::from_str("10")))
            .unwrap();

        assert_eq!(
            IncrCommand { key: "c".into() }.execute(&mut store).unwrap(),
            Value::Int(11)
        );
    }

    #[test]
    fn test_incr_twice_sequential() {
        let mut store = mem_store();
        let cmd = IncrCommand { key: "c".into() };

        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(1));
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(2));
    }

    #[test]
    fn test_incr_at_i64_max_returns_error() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Int(i64::MAX))
            .unwrap();

        let result = IncrCommand { key: "c".into() }.execute(&mut store);

        assert!(result.is_err(), "INCR on i64::MAX should return an error");

        // Значение в store не должно измениться.
        assert_eq!(
            store.get(&Sds::from_str("c")).unwrap(),
            Some(Value::Int(i64::MAX)),
        );
    }

    #[test]
    fn test_incr_string_i64_max_error() {
        let mut store = mem_store();

        store
            .set(
                &Sds::from_str("c"),
                Value::Str("9223372036854775807".into()),
            )
            .unwrap();

        let result = IncrCommand { key: "c".into() }.execute(&mut store);

        assert!(result.is_err());
    }

    #[test]
    fn test_incr_non_integer_string_errors() {
        let mut store = mem_store();

        store
            .set(
                &Sds::from_str("c"),
                Value::Str(Sds::from_str("not_a_number")),
            )
            .unwrap();

        assert!(matches!(
            IncrCommand { key: "c".into() }.execute(&mut store),
            Err(StoreError::InvalidType)
        ));
    }

    #[test]
    fn test_incr_wrong_value_type_errors() {
        let mut store = mem_store();

        store.set(&Sds::from_str("c"), Value::Bool(true)).unwrap();

        assert!(matches!(
            IncrCommand { key: "c".into() }.execute(&mut store),
            Err(StoreError::InvalidType)
        ));
    }

    #[test]
    fn test_incrby_missing_key() {
        let mut store = mem_store();

        assert_eq!(
            IncrByCommand {
                key: "c".into(),
                increment: 5,
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(5),
        );
    }

    #[test]
    fn test_incrby_existing_value() {
        let mut store = mem_store();

        store.set(&Sds::from_str("c"), Value::Int(10)).unwrap();

        assert_eq!(
            IncrByCommand {
                key: "c".into(),
                increment: 5,
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(15)
        )
    }

    #[test]
    fn test_incrby_negative_acts_like_decr() {
        let mut store = mem_store();

        store.set(&Sds::from_str("c"), Value::Int(10)).unwrap();

        assert_eq!(
            IncrByCommand {
                key: "c".into(),
                increment: -3,
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(7)
        );
    }

    #[test]
    fn test_incrby_string_value_redis_compat() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Str(Sds::from_str("100")))
            .unwrap();

        assert_eq!(
            IncrByCommand {
                key: "c".into(),
                increment: 50,
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(150)
        );
    }

    #[test]
    fn test_incrby_overflow_returns_error() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Int(i64::MAX))
            .unwrap();

        assert!(IncrByCommand {
            key: "c".into(),
            increment: 1,
        }
        .execute(&mut store)
        .is_err(),);
    }

    #[test]
    fn test_decr_missing_key_starts_at_minus_one() {
        let mut store = mem_store();

        assert_eq!(
            DecrCommand { key: "c".into() }.execute(&mut store).unwrap(),
            Value::Int(-1),
        );
    }

    #[test]
    fn test_decr_existing_value() {
        let mut store = mem_store();

        store.set(&Sds::from_str("c"), Value::Int(0)).unwrap();

        assert_eq!(
            DecrCommand { key: "c".into() }.execute(&mut store).unwrap(),
            Value::Int(-1),
        );
    }

    #[test]
    fn test_decr_string_value_redis_compat() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Str(Sds::from_str("5")))
            .unwrap();

        assert_eq!(
            DecrCommand { key: "c".into() }.execute(&mut store).unwrap(),
            Value::Int(4),
        );
    }

    #[test]
    fn test_decr_at_i64_min_returns_error() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Int(i64::MIN))
            .unwrap();

        assert!(DecrCommand { key: "c".into() }.execute(&mut store).is_err());
    }

    #[test]
    fn test_decrby_missing_key() {
        let mut store = mem_store();

        assert_eq!(
            DecrByCommand {
                key: "c".into(),
                decrement: 3,
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(-3)
        );
    }

    #[test]
    fn test_decrby_existing_value() {
        let mut store = mem_store();

        store.set(&Sds::from_str("c"), Value::Int(10)).unwrap();

        assert_eq!(
            DecrByCommand {
                key: "c".into(),
                decrement: 3,
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(7),
        );
    }

    #[test]
    fn test_decrby_string_value_redis_compat() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Str(Sds::from_str("20")))
            .unwrap();

        assert_eq!(
            DecrByCommand {
                key: "c".into(),
                decrement: 7,
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(13),
        );
    }

    #[test]
    fn test_decrby_overflow_returns_error() {
        let mut store = mem_store();

        store
            .set(&Sds::from_str("c"), Value::Int(i64::MIN))
            .unwrap();

        assert!(DecrByCommand {
            key: "c".into(),
            decrement: 1,
        }
        .execute(&mut store)
        .is_err(),);
    }

    #[test]
    fn test_decrby_invalid_type() {
        let mut store = mem_store();

        store
            .set(
                &Sds::from_str("c"),
                Value::Str(Sds::from_str("not_a_number")),
            )
            .unwrap();

        assert!(matches!(
            DecrByCommand {
                key: "c".into(),
                decrement: 1,
            }
            .execute(&mut store),
            Err(StoreError::InvalidType)
        ));
    }

    #[test]
    fn test_incr_command() {
        let mut store = mem_store();
        let cmd = IncrCommand {
            key: "counter".into(),
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(1));
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(2));
    }

    #[test]
    fn test_incrby_command() {
        let mut store = mem_store();
        let cmd = IncrByCommand {
            key: "counter".into(),
            increment: 5,
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(5));
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(10));
    }

    #[test]
    fn test_decr_command() {
        let mut store = mem_store();
        let cmd = DecrCommand {
            key: "counter".into(),
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(-1));
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(-2));
    }

    #[test]
    fn test_decrby_command() {
        let mut store = mem_store();
        let cmd = DecrByCommand {
            key: "counter".into(),
            decrement: 3,
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(-3));
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(-6));
    }

    #[test]
    fn test_invalid_type_for_incr() {
        let mut store = mem_store();
        store
            .set(
                &Sds::from_str("counter"),
                Value::Str(Sds::from_str("string")),
            )
            .unwrap();
        assert!(IncrCommand {
            key: "counter".into()
        }
        .execute(&mut store)
        .is_err());
    }

    #[test]
    fn test_invalid_type_for_decr() {
        let mut store = mem_store();
        store
            .set(
                &Sds::from_str("counter"),
                Value::Str(Sds::from_str("string")),
            )
            .unwrap();
        assert!(DecrCommand {
            key: "counter".into()
        }
        .execute(&mut store)
        .is_err());
    }
}
