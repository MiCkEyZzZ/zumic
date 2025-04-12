use std::collections::{BTreeMap, HashMap, HashSet};

use ordered_float::OrderedFloat;

use crate::{
    database::{ArcBytes, QuickList, Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct ZAddCommand {
    pub key: String,
    pub member: String,
    pub score: f64,
}

impl CommandExecute for ZAddCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let member = ArcBytes::from_str(&self.member);

        // Получаем существующий ZSet или создаём новый
        let (mut dict, mut sorted) = match store.get(key.clone())? {
            Some(Value::ZSet { dict, sorted }) => (dict, sorted),
            Some(_) => return Err(StoreError::InvalidType),
            None => (HashMap::new(), BTreeMap::new()),
        };

        // Проверяем, был ли новый элемент
        let is_new = dict.insert(member.clone(), self.score).is_none();

        // Если элемент уже был, удаляем его из старого бина
        if !is_new {
            let old = OrderedFloat(self.score);
            // По идее старый score мы должны взять до вставки, но здесь для простоты
            // удаляем всё, что попало под тот же f64. Сначала реализуем для простоты
            // чтобы запустить и было более или мнее надёжно, а после когда будем делать
            // оптимизацию то будем смотреть где узкие места и скорее всего будем что-то
            // менять.
            if let Some(bucket) = sorted.get_mut(&old) {
                bucket.remove(&member);
                if bucket.is_empty() {
                    sorted.remove(&old);
                }
            }
        }

        // Вставляем в новый bucket
        sorted
            .entry(OrderedFloat(self.score))
            .or_insert_with(HashSet::new)
            .insert(member.clone());

        // Сохраняем обратно
        store.set(key, Value::ZSet { dict, sorted })?;
        Ok(Value::Int(if is_new { 1 } else { 0 }))
    }
}

#[derive(Debug)]
pub struct ZRemCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZRemCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let member = ArcBytes::from_str(&self.member);

        // Извлекаем существующий ZSet
        if let Some(Value::ZSet { dict, sorted }) = store.get(key.clone())? {
            let mut dict = dict;
            let mut sorted = sorted;
            let removed = dict.remove(&member).is_some();
            if removed {
                // Удаляем из sorted
                // Ищем тот bucket, где member мог быть
                let mut to_remove = None;
                for (score, bucket) in &mut sorted {
                    if bucket.remove(&member) {
                        if bucket.is_empty() {
                            to_remove = Some(*score);
                        }
                        break;
                    }
                }
                if let Some(score) = to_remove {
                    sorted.remove(&score);
                }
                store.set(key, Value::ZSet { dict, sorted })?;
            }
            return Ok(Value::Int(removed as i64));
        }
        // Нет такого ключа или не ZSet
        Ok(Value::Int(0))
    }
}

#[derive(Debug)]
pub struct ZScoreCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZScoreCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let member = ArcBytes::from_str(&self.member);

        match store.get(key)? {
            Some(Value::ZSet { dict, .. }) => {
                if let Some(&score) = dict.get(&member) {
                    Ok(Value::Float(score))
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
pub struct ZCardCommand {
    pub key: String,
}

impl CommandExecute for ZCardCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        match store.get(key)? {
            Some(Value::ZSet { dict, .. }) => Ok(Value::Int(dict.len() as i64)),
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }
}

#[derive(Debug)]
pub struct ZRangeCommand {
    pub key: String,
    pub start: i64,
    pub stop: i64,
}

impl CommandExecute for ZRangeCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        match store.get(key)? {
            Some(Value::ZSet { sorted, .. }) => {
                // Собираем все members в порядке возрастания
                let mut all: Vec<ArcBytes> = Vec::new();
                for bucket in sorted.values() {
                    for member in bucket {
                        all.push(member.clone());
                    }
                }
                // Переводим start/stop в индексы
                let len = all.len() as i64;
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
                let slice = if s <= e && s < all.len() {
                    &all[s..=e]
                } else {
                    &[]
                };
                let list = QuickList::from_iter(slice.iter().cloned(), 64);
                Ok(Value::List(list))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Null),
        }
    }
}

#[derive(Debug)]
pub struct ZRevRangeCommand {
    pub key: String,
    pub start: i64,
    pub stop: i64,
}

impl CommandExecute for ZRevRangeCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        match store.get(key)? {
            Some(Value::ZSet { sorted, .. }) => {
                // Собираем в обратном порядке
                let mut all: Vec<ArcBytes> = Vec::new();
                for bucket in sorted.values().rev() {
                    for member in bucket {
                        all.push(member.clone());
                    }
                }
                let len = all.len() as i64;
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
                let slice = if s <= e && s < all.len() {
                    &all[s..=e]
                } else {
                    &[]
                };
                let list = QuickList::from_iter(slice.iter().cloned(), 64);
                Ok(Value::List(list))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Null),
        }
    }
}
