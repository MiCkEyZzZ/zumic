use crate::{
    database::{ArcBytes, Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct StrLenCommand {
    pub key: String,
}

impl CommandExecute for StrLenCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        if let Some(value) = store.get(key)? {
            if let Value::Str(ref s) = value {
                Ok(Value::Int(s.len() as i64))
            } else {
                Err(StoreError::InvalidType)
            }
        } else {
            Ok(Value::Int(0))
        }
    }
}

#[derive(Debug)]
pub struct AppendCommand {
    pub key: String,
    pub value: String,
}

impl CommandExecute for AppendCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let append_value = ArcBytes::from_str(&self.value);

        if let Some(mut existing_value) = store.get(key.clone())? {
            if let Value::Str(ref mut s) = existing_value {
                // Клонируем s для работы с данными и добавляем новые байты
                let mut updated_value = s.to_vec();
                updated_value.extend_from_slice(&append_value.to_vec()); // Добавляем новые байты

                // Сохраняем обновленное значение
                store.set(key, Value::Str(ArcBytes::from_vec(updated_value.clone())))?;
                return Ok(Value::Int(updated_value.len() as i64)); // Возвращаем длину обновленной строки
            } else {
                return Err(StoreError::InvalidType); // Ошибка, если значение не строка
            }
        }

        // Если строки не было, создаем новую строку
        store.set(key, Value::Str(append_value.clone()))?;
        Ok(Value::Int(append_value.len() as i64)) // Возвращаем длину новой строки
    }
}

#[derive(Debug)]
pub struct GetRangeCommand {
    pub key: String,
    pub start: i64,
    pub end: i64,
}

impl CommandExecute for GetRangeCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        if let Some(value) = store.get(key)? {
            if let Value::Str(ref s) = value {
                let start = self.start as usize;
                let end = self.end as usize;
                let sliced = s.slice(start..end);
                return Ok(Value::Str(sliced));
            } else {
                return Err(StoreError::InvalidType);
            }
        }
        Ok(Value::Null)
    }
}
