//! Команды для работы с хэш-таблицами (HASH) в Zumic.
//!
//! Реализует команды HSET, HGET, HDEL, HGETALL для управления полями
//! и значениями в хешах.
//! Каждая команда реализует трейт [`CommandExecute`].

use crate::{CommandExecute, QuickList, Sds, SmartHash, StorageEngine, StoreError, Value};

/// Команда HSET — устанавливает значение поля в хеше.
///
/// Формат: `HSET key field value`
///
/// # Поля
/// * `key` — ключ хеша.
/// * `field` — имя поля.
/// * `value` — значение поля.
///
/// # Возвращает
/// 1, если поле было добавлено или обновлено.
#[derive(Debug)]
pub struct HSetCommand {
    pub key: String,
    pub field: String,
    pub value: String,
}

impl CommandExecute for HSetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let field = Sds::from_str(&self.field);
        let value = Sds::from_str(&self.value);

        match store.get(&key)? {
            Some(Value::Hash(mut smart_hash)) => {
                smart_hash.insert(field.clone(), value.clone());
                store.set(&key, Value::Hash(smart_hash))?;
                Ok(Value::Int(1))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                let mut smart_hash = SmartHash::new();
                smart_hash.insert(field.clone(), value.clone());
                store.set(&key, Value::Hash(smart_hash))?;
                Ok(Value::Int(1))
            }
        }
    }
}

/// Команда HGET — получает значение поля из хеша.
///
/// Формат: `HGET key field`
///
/// # Поля
/// * `key` — ключ хеша.
/// * `field` — имя поля.
///
/// # Возвращает
/// Значение поля или `Null`, если поле не найдено.
#[derive(Debug)]
pub struct HGetCommand {
    pub key: String,
    pub field: String,
}

impl CommandExecute for HGetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let field = Sds::from_str(&self.field);

        if let Some(Value::Hash(ref mut smart_hash)) = store.get(&key)? {
            if let Some(value) = smart_hash.get(&field) {
                return Ok(Value::Str(value.clone()));
            } else {
                return Ok(Value::Null);
            }
        }
        Ok(Value::Null)
    }
}

/// Команда HDEL — удаляет поле из хеша.
///
/// Формат: `HDEL key field`
///
/// # Поля
/// * `key` — ключ хеша.
/// * `field` — имя поля.
///
/// # Возвращает
/// 1, если поле было удалено, 0 — если поле не найдено.
#[derive(Debug)]
pub struct HDelCommand {
    pub key: String,
    pub field: String,
}

impl CommandExecute for HDelCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let field = Sds::from_str(&self.field);

        if let Some(Value::Hash(mut smart_hash)) = store.get(&key)? {
            let removed = smart_hash.remove(&field);
            if removed {
                store.set(&key, Value::Hash(smart_hash))?;
                return Ok(Value::Int(1));
            }
            return Ok(Value::Int(0));
        }
        Ok(Value::Int(0))
    }
}

/// Команда HGETALL — получает все поля и значения хеша.
///
/// Формат: `HGETALL key`
///
/// # Поля
/// * `key` — ключ хеша.
///
/// # Возвращает
/// Список всех полей и значений (как чередующиеся элементы списка) или `Null`, если хэш не найден.
#[derive(Debug)]
pub struct HGetAllCommand {
    pub key: String,
}

impl CommandExecute for HGetAllCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        if let Some(Value::Hash(ref mut smart_hash)) = store.get(&key)? {
            let result: QuickList<Sds> = QuickList::from_iter(
                smart_hash.iter().flat_map(|(k, v)| {
                    [Sds::from_str(&k.to_string()), Sds::from_str(&v.to_string())]
                }),
                64,
            );
            return Ok(Value::List(result));
        }
        Ok(Value::Null)
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

    /// Тестирует установку поля в хэш с помощью HSet и получение его с помощью HGet
    #[test]
    fn test_hset_and_hget() {
        let mut store = create_store();

        // Устанавливаем поле хеша с помощью HSetCommand.
        let hset_cmd = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };

        let result = hset_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap(), &Value::Int(1));

        // Получаем значение этого поля.
        let hget_cmd = HGetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
        };

        let get_result = hget_cmd.execute(&mut store);
        assert!(get_result.is_ok());
        assert_eq!(
            get_result.as_ref().unwrap(),
            &Value::Str(Sds::from_str("value1"))
        );
    }

    /// Проверяет, что HGet возвращает Null при запросе несуществующего поля
    #[test]
    fn test_hget_nonexistent_field() {
        let mut store = create_store();

        // Сначала установим одно поле
        let hset_cmd = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };
        hset_cmd.execute(&mut store).unwrap();

        // Пытаемся получить значение несуществующего поля
        let hget_cmd = HGetCommand {
            key: "hash".to_string(),
            field: "nonexistent".to_string(),
        };
        let get_result = hget_cmd.execute(&mut store);

        match get_result {
            Ok(Value::Null) => {}
            other => panic!("Expected Ok(Value::Null), got {other:?}"),
        }
    }

    /// Проверяет, что HDel удаляет поле, и что оно действительно исчезает из хеша
    #[test]
    fn test_hdel_command() {
        let mut store = create_store();

        // Сначала установим одно поле
        let hset_cmd = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };
        hset_cmd.execute(&mut store).unwrap();

        // Удаляем это поле
        let hdel_cmd = HDelCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
        };
        let del_result = hdel_cmd.execute(&mut store);
        match del_result {
            Ok(Value::Int(1)) => {}
            other => panic!("Expected Ok(Value::Int(1)), got {other:?}"),
        }

        // Проверяем, что поле действительно удалено
        let hget_cmd = HGetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
        };
        let get_result = hget_cmd.execute(&mut store);
        match get_result {
            Ok(Value::Null) => {}
            other => panic!("Expected Ok(Value::Null), got {other:?}"),
        }
    }

    /// Проверяет, что HGetAll возвращает все поля и значения хеша в виде списка строк "поле: значение"
    #[test]
    fn test_hgetall_command() {
        let mut store = create_store();

        let hset_cmd1 = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };
        let hset_cmd2 = HSetCommand {
            key: "hash".to_string(),
            field: "field2".to_string(),
            value: "value2".to_string(),
        };
        hset_cmd1.execute(&mut store).unwrap();
        hset_cmd2.execute(&mut store).unwrap();

        let hgetall_cmd = HGetAllCommand {
            key: "hash".to_string(),
        };
        let result = hgetall_cmd.execute(&mut store).unwrap();

        if let Value::List(quicklist) = result {
            let items: Vec<String> = quicklist
                .iter()
                .map(|ab| ab.as_str().unwrap_or("").to_string())
                .collect();

            // Собираем пары
            let mut pairs = vec![];
            for chunk in items.chunks(2) {
                if let [key, val] = chunk {
                    pairs.push((key.clone(), val.clone()));
                }
            }

            pairs.sort();

            assert_eq!(
                pairs,
                vec![
                    ("field1".to_string(), "value1".to_string()),
                    ("field2".to_string(), "value2".to_string())
                ]
            );
        } else {
            panic!("Expected Value::List from HGetAllCommand");
        }
    }
}
