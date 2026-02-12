use std::iter::empty;

use rand::seq::IteratorRandom;

use crate::{CommandExecute, QuickList, Sds, SmartHash, StorageEngine, StoreError, Value};

/// Команда HSET — устанавливает одно или несколько полей хеша.
#[derive(Debug)]
pub struct HSetCommand {
    pub key: String,
    pub entries: Vec<(String, String)>,
}

impl CommandExecute for HSetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Hash(mut sh)) => {
                let mut added = 0;

                for (f, v) in &self.entries {
                    let field = Sds::from_str(f);
                    let value = Sds::from_str(v);

                    if sh.insert(field, value) {
                        added += 1;
                    }
                }

                store.set(&key, Value::Hash(sh))?;
                Ok(Value::Int(added))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                let mut sh = SmartHash::new();
                let mut added = 0;

                for (f, v) in &self.entries {
                    let field = Sds::from_str(f);
                    let value = Sds::from_str(v);

                    if sh.insert(field, value) {
                        added += 1;
                    }
                }

                store.set(&key, Value::Hash(sh))?;
                Ok(Value::Int(added))
            }
        }
    }

    fn command_name(&self) -> &'static str {
        "HSET"
    }
}

/// Команда HGET — получает значение поля из хеша.
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

        match store.get(&key)? {
            Some(Value::Hash(sh)) => {
                if let Some(value) = sh.get(&field) {
                    Ok(Value::Str(value.clone()))
                } else {
                    Ok(Value::Null)
                }
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Null),
        }
    }

    fn command_name(&self) -> &'static str {
        "HGET"
    }
}

/// Команда HMGET - получает значения нескольких полей хеша.
#[derive(Debug)]
pub struct HmGetCommand {
    pub key: String,
    pub fields: Vec<String>,
}

impl CommandExecute for HmGetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Hash(sh)) => {
                let result = self
                    .fields
                    .iter()
                    .map(|f| {
                        let field = Sds::from_str(f);
                        match sh.get(&field) {
                            Some(v) => Value::Str(v.clone()),
                            None => Value::Null,
                        }
                    })
                    .collect();
                Ok(Value::Array(result))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Array(vec![Value::Null; self.fields.len()])),
        }
    }

    fn command_name(&self) -> &'static str {
        "HMGET"
    }
}

/// Команда HDEL — удаляет поле из хеша.
#[derive(Debug)]
pub struct HDelCommand {
    pub key: String,
    pub fields: Vec<String>,
}

impl CommandExecute for HDelCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Hash(mut sh)) => {
                let mut removed = 0;

                for f in &self.fields {
                    let field = Sds::from_str(f);
                    if sh.remove(&field) {
                        removed += 1;
                    }
                }

                // если контейнер пуст - можно опционально удалить ключ из store
                if sh.is_empty() {
                    store.del(&key)?;
                } else {
                    store.set(&key, Value::Hash(sh))?;
                }
                Ok(Value::Int(removed))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }

    fn command_name(&self) -> &'static str {
        "HDEL"
    }
}

/// Команда HEXISTS — проверяет существование поля в хеше.
#[derive(Debug)]
pub struct HExistsCommand {
    pub key: String,
    pub field: String,
}

impl CommandExecute for HExistsCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let field = Sds::from_str(&self.field);

        match store.get(&key)? {
            Some(Value::Hash(sh)) => Ok(Value::Int(i64::from(sh.contains(&field)))),
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }

    fn command_name(&self) -> &'static str {
        "HEXISTS"
    }
}

/// Команда HLEN — возвращает количество полей в хеше.
#[derive(Debug)]
pub struct HLenCommand {
    pub key: String,
}

impl CommandExecute for HLenCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Hash(sh)) => Ok(Value::Int(sh.len() as i64)),
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }

    fn command_name(&self) -> &'static str {
        "HLEN"
    }
}

/// Команда HKEYS — возвращает список всех полей хеша.
#[derive(Debug)]
pub struct HKeysCommand {
    pub key: String,
}

impl CommandExecute for HKeysCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Hash(sh)) => {
                let keys = sh.keys();
                Ok(Value::List(QuickList::from_iter(keys, 64)))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::List(QuickList::from_iter(empty(), 64))),
        }
    }

    fn command_name(&self) -> &'static str {
        "HKEYS"
    }
}

/// Команда HVALS — возвращает список всех значений хеша.
#[derive(Debug)]
pub struct HValsCommand {
    pub key: String,
}

impl CommandExecute for HValsCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Hash(sh)) => {
                let vals = sh.values();
                Ok(Value::List(QuickList::from_iter(vals, 64)))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::List(QuickList::from_iter(empty(), 64))),
        }
    }

    fn command_name(&self) -> &'static str {
        "HVALS"
    }
}

/// Команда HGETALL — получает все поля и значения хеша.
///
/// Ключи сортируются по алфавиту для предсказуемого порядка (важно для тестов).
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

        match store.get(&key)? {
            Some(Value::Hash(mut sh)) => {
                // сортируем ключи для предсказуемого порядка
                let mut entries: Vec<_> = sh.iter().collect();
                entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                let result: QuickList<Sds> = QuickList::from_iter(
                    entries
                        .into_iter()
                        .flat_map(|(k, v)| [k.clone(), v.clone()]),
                    64,
                );
                Ok(Value::List(result))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Null),
        }
    }

    fn command_name(&self) -> &'static str {
        "HGETALL"
    }
}

/// Команда HRANDFIELD — возвращает одно или несколько случайных полей хеша.
/// Если count отрицательный — возвращает ровно |count| элементов, повторения
/// возможны.
#[derive(Debug)]
pub struct HRandFieldCommand {
    pub key: String,
    pub count: Option<i64>,
    pub with_values: bool,
}

impl CommandExecute for HRandFieldCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::Hash(sh)) => {
                let entries = sh.entries();
                let mut rng = rand::thread_rng();

                match self.count {
                    Some(count) if count < 0 => {
                        let n = count.unsigned_abs() as usize;
                        let items: Vec<_> = (0..n)
                            .filter_map(|_| entries.iter().choose(&mut rng).cloned())
                            .collect();

                        let flat: Vec<Sds> = if self.with_values {
                            items.into_iter().flat_map(|(f, v)| [f, v]).collect()
                        } else {
                            items.into_iter().map(|(f, _)| f).collect()
                        };

                        Ok(Value::List(QuickList::from_iter(flat, 64)))
                    }
                    Some(count) => {
                        let n = (count as usize).min(entries.len());
                        let items: Vec<_> = entries.into_iter().choose_multiple(&mut rng, n);

                        let flat: Vec<Sds> = if self.with_values {
                            items.into_iter().flat_map(|(f, v)| [f, v]).collect()
                        } else {
                            items.into_iter().map(|(f, _)| f).collect()
                        };

                        Ok(Value::List(QuickList::from_iter(flat, 64)))
                    }
                    None => match entries.into_iter().choose(&mut rng) {
                        Some((field, _)) => Ok(Value::Str(field)),
                        None => Ok(Value::Null),
                    },
                }
            }
            Some(_) => Err(StoreError::InvalidType),
            None => match self.count {
                Some(_) => Ok(Value::List(QuickList::from_iter(empty(), 64))),
                None => Ok(Value::Null),
            },
        }
    }

    fn command_name(&self) -> &'static str {
        "HRANDFIELD"
    }
}

/// Команда HINCRBY — автомарно увеличивает целочисленное поле хеша.
#[derive(Debug)]
pub struct HIncrByCommand {
    pub key: String,
    pub field: String,
    pub increment: i64,
}

impl CommandExecute for HIncrByCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let field = Sds::from_str(&self.field);

        // Получаем хеш или создаём новый
        let mut hash = match store.get(&key)? {
            Some(Value::Hash(h)) => h,
            Some(_) => return Err(StoreError::InvalidType),
            None => SmartHash::new(),
        };

        // Получаем текущее значение (или 0 если не существует)
        let current: i64 = match hash.get(&field) {
            Some(v) => {
                let s = v.as_str().map_err(|_| StoreError::InvalidValue)?;
                s.parse().map_err(|_| StoreError::InvalidValue)?
            }
            None => 0,
        };

        // Выполняем checked_add для обнаружения overflow
        let new_value = current
            .checked_add(self.increment)
            .ok_or(StoreError::Overflow)?;

        // Сохраняем новое значение
        let new_str = new_value.to_string();
        hash.insert(field, Sds::from_str(&new_str));
        store.set(&key, Value::Hash(hash))?;

        Ok(Value::Int(new_value))
    }

    fn command_name(&self) -> &'static str {
        "HINCRBY"
    }
}

/// Команда HINCRBYFLOAT — атомарно увеличивает поле с плавающей точкой.
#[derive(Debug)]
pub struct HIncrByFloatCommand {
    pub key: String,
    pub field: String,
    pub increment: f64,
}

impl CommandExecute for HIncrByFloatCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let field = Sds::from_str(&self.field);

        let mut hash = match store.get(&key)? {
            Some(Value::Hash(h)) => h,
            Some(_) => return Err(StoreError::InvalidType),
            None => SmartHash::new(),
        };

        // Получаем текущее значение (или 0.0 если не существует)
        let current: f64 = match hash.get(&field) {
            Some(v) => {
                let s = v.as_str().map_err(|_| StoreError::InvalidValue)?;
                s.parse().map_err(|_| StoreError::InvalidValue)?
            }
            None => 0.0,
        };

        if self.increment.is_nan() {
            return Err(StoreError::InvalidValue);
        }

        let new_value = current + self.increment;

        if !new_value.is_finite() {
            return Err(StoreError::InvalidValue);
        }

        // Сохраняем новое значение
        hash.insert(field, Sds::from_str(&new_value.to_string()));
        store.set(&key, Value::Hash(hash))?;

        Ok(Value::Float(new_value))
    }

    fn command_name(&self) -> &'static str {
        "HINCRBYFLOAT"
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::InMemoryStore;

    // Вспомогательная функция для создания нового хранилища в памяти.
    fn create_store() -> StorageEngine {
        StorageEngine::Memory(InMemoryStore::new())
    }

    // Вспомогательная ф-я для создания хеша с тремя полями
    fn setup_hash(store: &mut StorageEngine) {
        HSetCommand {
            key: "user:1".into(),
            entries: vec![
                ("name".into(), "Anton".into()),
                ("age".into(), "38".into()),
                ("city".into(), "Kungur".into()),
            ],
        }
        .execute(store)
        .unwrap();
    }

    /// Тестирует установку поля в хэш с помощью HSet и получение его с помощью
    /// HGet
    #[test]
    fn test_hset_and_hget() {
        let mut store = create_store();

        // Устанавливаем поле хеша с помощью HSetCommand.
        let hset_cmd = HSetCommand {
            key: "hash".into(),
            entries: vec![("field1".into(), "value1".into())],
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
            entries: vec![("field1".to_string(), "value1".to_string())],
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

    /// Проверяет, что HDel удаляет поле, и что оно действительно исчезает из
    /// хеша
    #[test]
    fn test_hdel_command() {
        let mut store = create_store();

        // Сначала установим одно поле
        let hset_cmd = HSetCommand {
            key: "hash".to_string(),
            entries: vec![("field1".to_string(), "value1".to_string())],
        };
        hset_cmd.execute(&mut store).unwrap();

        // Удаляем это поле
        let hdel_cmd = HDelCommand {
            key: "hash".to_string(),
            fields: vec!["field1".to_string()],
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

    /// Проверяет, что HGetAll возвращает все поля и значения хеша в виде списка
    /// строк "поле: значение"
    #[test]
    fn test_hgetall_command() {
        let mut store = create_store();

        let hset_cmd1 = HSetCommand {
            key: "hash".to_string(),
            entries: vec![("field1".to_string(), "value1".to_string())],
        };
        let hset_cmd2 = HSetCommand {
            key: "hash".to_string(),
            entries: vec![("field2".to_string(), "value2".to_string())],
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

    #[test]
    fn test_hmget_mixed() {
        let mut store = create_store();
        setup_hash(&mut store);

        let res = HmGetCommand {
            key: "user:1".into(),
            fields: vec!["name".into(), "missing".into(), "city".into()],
        }
        .execute(&mut store)
        .unwrap();

        assert_eq!(
            res,
            Value::Array(vec![
                Value::Str(Sds::from_str("Anton")),
                Value::Null,
                Value::Str(Sds::from_str("Kungur")),
            ])
        );
    }

    #[test]
    fn test_hmget_missing_key() {
        let mut store = create_store();

        let res = HmGetCommand {
            key: "ghost".into(),
            fields: vec!["f1".into(), "f2".into()],
        }
        .execute(&mut store)
        .unwrap();

        assert_eq!(res, Value::Array(vec![Value::Null, Value::Null]));
    }

    #[test]
    fn test_hexists() {
        let mut store = create_store();
        setup_hash(&mut store);

        assert_eq!(
            HExistsCommand {
                key: "user:1".into(),
                field: "age".into()
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(1)
        );
        assert_eq!(
            HExistsCommand {
                key: "user:1".into(),
                field: "phone".into(),
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            HExistsCommand {
                key: "ghost".into(),
                field: "age".into(),
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(0)
        );
    }

    #[test]
    fn test_hlen() {
        let mut store = create_store();
        setup_hash(&mut store);

        assert_eq!(
            HLenCommand {
                key: "user:1".into()
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(3)
        );
        assert_eq!(
            HLenCommand {
                key: "ghost".into(),
            }
            .execute(&mut store)
            .unwrap(),
            Value::Int(0)
        );
    }

    #[test]
    fn test_hkeys() {
        let mut store = create_store();
        setup_hash(&mut store);

        let keys_com = HKeysCommand {
            key: "user:1".into(),
        }
        .execute(&mut store)
        .unwrap();

        if let Value::List(ql) = keys_com {
            let keys: Vec<String> = ql.iter().map(|s| s.to_string()).collect();
            assert_eq!(keys.len(), 3);
            assert!(keys.contains(&"name".to_string()));
            assert!(keys.contains(&"age".to_string()));
            assert!(keys.contains(&"city".to_string()));
        } else {
            panic!("expected Value::List from HKEYS");
        }
    }

    #[test]
    fn test_hvals() {
        let mut store = create_store();
        setup_hash(&mut store);

        let vals_com = HValsCommand {
            key: "user:1".into(),
        }
        .execute(&mut store)
        .unwrap();

        if let Value::List(ql) = vals_com {
            let vals: Vec<String> = ql.iter().map(|s| s.to_string()).collect();
            assert_eq!(vals.len(), 3);
            assert!(vals.contains(&"Anton".to_string()));
            assert!(vals.contains(&"38".to_string()));
            assert!(vals.contains(&"Kungur".to_string()));
        } else {
            panic!("expected Value::List from HVALS");
        }
    }

    #[test]
    fn test_hrandfield_single() {
        let mut store = create_store();
        setup_hash(&mut store);

        let res = HRandFieldCommand {
            key: "user:1".into(),
            count: None,
            with_values: false,
        }
        .execute(&mut store)
        .unwrap();

        assert!(matches!(res, Value::Str(_)));

        let res2 = HRandFieldCommand {
            key: "ghost".into(),
            count: None,
            with_values: false,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(res2, Value::Null);
    }

    #[test]
    fn test_hrandfield_count_positive() {
        let mut store = create_store();
        setup_hash(&mut store);

        let rand_field = HRandFieldCommand {
            key: "user:1".into(),
            count: Some(2),
            with_values: false,
        }
        .execute(&mut store)
        .unwrap();

        if let Value::List(ql) = rand_field {
            assert!(ql.len() <= 2);
        } else {
            panic!("expected Value::List");
        }
    }

    #[test]
    fn test_hrandfield_count_negative() {
        let mut store = create_store();
        setup_hash(&mut store);

        let rand_field_ng = HRandFieldCommand {
            key: "user:1".into(),
            count: Some(-3),
            with_values: false,
        }
        .execute(&mut store)
        .unwrap();

        if let Value::List(ql) = rand_field_ng {
            assert_eq!(ql.len(), 3);
        } else {
            panic!("expected Value::List");
        }
    }

    #[test]
    fn test_hrandfield_with_values() {
        let mut store = create_store();
        setup_hash(&mut store);

        let rand_field_ps = HRandFieldCommand {
            key: "user:1".into(),
            count: Some(2),
            with_values: true,
        }
        .execute(&mut store)
        .unwrap();

        if let Value::List(ql) = rand_field_ps {
            assert_eq!(ql.len() % 2, 0);
            assert!(ql.len() <= 4);
        } else {
            panic!("expected Value::List");
        }
    }

    #[test]
    fn test_wrong_type_error() {
        let mut store = create_store();

        // Создаём строку
        store
            .set(&Sds::from_str("str"), Value::Str(Sds::from_str("hello")))
            .unwrap();

        let res = HGetCommand {
            key: "str".into(),
            field: "f".into(),
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::InvalidType)));
    }

    #[test]
    fn test_hincrby_new_field() {
        let mut store = create_store();

        let res = HIncrByCommand {
            key: "counter".into(),
            field: "views".into(),
            increment: 10,
        }
        .execute(&mut store)
        .unwrap();

        assert_eq!(res, Value::Int(10));

        // Проверяем что значение сохранилось
        let get = HGetCommand {
            key: "counter".into(),
            field: "views".into(),
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(get, Value::Str(Sds::from_str("10")));
    }

    #[test]
    fn test_hincrby_existing_field() {
        let mut store = create_store();

        // Устанавливаем начальное значение
        HSetCommand {
            key: "stats".into(),
            entries: vec![("hits".into(), "100".into())],
        }
        .execute(&mut store)
        .unwrap();

        // Инкремент
        let res = HIncrByCommand {
            key: "stats".into(),
            field: "hits".into(),
            increment: 5,
        }
        .execute(&mut store)
        .unwrap();

        assert_eq!(res, Value::Int(105));
    }

    #[test]
    fn test_hincrby_negative() {
        let mut store = create_store();

        HSetCommand {
            key: "balance".into(),
            entries: vec![("amount".into(), "1000".into())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByCommand {
            key: "balance".into(),
            field: "amount".into(),
            increment: -250,
        }
        .execute(&mut store)
        .unwrap();

        assert_eq!(res, Value::Int(750));
    }

    #[test]
    fn test_hincrby_invalid_value() {
        let mut store = create_store();

        HSetCommand {
            key: "user".into(),
            entries: vec![("name".into(), "Anton".into())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByCommand {
            key: "user".into(),
            field: "name".into(),
            increment: 1,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::InvalidValue)));
    }

    #[test]
    fn test_hincrby_overflow() {
        let mut store = create_store();

        HSetCommand {
            key: "big".into(),
            entries: vec![("num".into(), i64::MAX.to_string())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByCommand {
            key: "big".into(),
            field: "num".into(),
            increment: 1,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::Overflow)));
    }

    #[test]
    fn test_hincrby_underflow() {
        let mut store = create_store();

        HSetCommand {
            key: "small".into(),
            entries: vec![("num".into(), i64::MIN.to_string())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByCommand {
            key: "small".into(),
            field: "num".into(),
            increment: -1,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::Overflow)));
    }

    #[test]
    fn test_hincrby_wrong_type() {
        let mut store = create_store();

        store
            .set(&Sds::from_str("str"), Value::Str(Sds::from_str("hello")))
            .unwrap();

        let res = HIncrByCommand {
            key: "str".into(),
            field: "f".into(),
            increment: 1,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::InvalidType)));
    }

    #[test]
    fn test_hincrbyfloat_new_field() {
        let mut store = create_store();

        let res = HIncrByFloatCommand {
            key: "metrics".into(),
            field: "pi".into(),
            increment: PI,
        }
        .execute(&mut store)
        .unwrap();

        assert_eq!(res, Value::Float(PI));
    }

    #[test]
    fn test_hincrbyfloat_existing() {
        let mut store = create_store();

        HSetCommand {
            key: "temp".into(),
            entries: vec![("celsius".into(), "20.5".into())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByFloatCommand {
            key: "temp".into(),
            field: "celsius".into(),
            increment: 2.3,
        }
        .execute(&mut store)
        .unwrap();

        if let Value::Float(v) = res {
            assert!((v - 22.8).abs() < 0.0001);
        } else {
            panic!("expected Float");
        }
    }

    /// HINCRBYFLOAT с отрицательным инкрементом
    #[test]
    fn test_hincrbyfloat_negative() {
        let mut store = create_store();

        HSetCommand {
            key: "price".into(),
            entries: vec![("usd".into(), "99.99".into())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByFloatCommand {
            key: "price".into(),
            field: "usd".into(),
            increment: -10.0,
        }
        .execute(&mut store)
        .unwrap();

        if let Value::Float(v) = res {
            assert!((v - 89.99).abs() < 0.0001);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn test_hincrbyfloat_invalid_value() {
        let mut store = create_store();

        HSetCommand {
            key: "data".into(),
            entries: vec![("text".into(), "not_a_number".into())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByFloatCommand {
            key: "data".into(),
            field: "text".into(),
            increment: 1.5,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::InvalidValue)));
    }

    #[test]
    fn test_hincrbyfloat_nan() {
        let mut store = create_store();

        HSetCommand {
            key: "nan".into(),
            entries: vec![("val".into(), "0.0".into())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByFloatCommand {
            key: "nan".into(),
            field: "val".into(),
            increment: f64::NAN,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::InvalidValue)));
    }

    #[test]
    fn test_hincrbyfloat_infinity() {
        let mut store = create_store();

        HSetCommand {
            key: "inf".into(),
            entries: vec![("val".into(), f64::MAX.to_string())],
        }
        .execute(&mut store)
        .unwrap();

        let res = HIncrByFloatCommand {
            key: "inf".into(),
            field: "val".into(),
            increment: f64::MAX,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::InvalidValue)));
    }

    #[test]
    fn test_hincrbyfloat_wrong_type() {
        let mut store = create_store();

        store
            .set(
                &Sds::from_str("list"),
                Value::List(QuickList::from_iter(empty(), 64)),
            )
            .unwrap();

        let res = HIncrByFloatCommand {
            key: "list".into(),
            field: "f".into(),
            increment: 1.0,
        }
        .execute(&mut store);

        assert!(matches!(res, Err(StoreError::InvalidType)));
    }
}
