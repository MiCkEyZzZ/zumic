use ordered_float::OrderedFloat;

use crate::{CommandExecute, Dict, QuickList, Sds, SkipList, StorageEngine, StoreError, Value};

/// Команда ZADD — добавляет элемент с баллом (score) в упорядоченное множество.
#[derive(Debug)]
pub struct ZAddCommand {
    pub key: String,
    pub member: String,
    pub score: f64,
}

impl CommandExecute for ZAddCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        // Получить существующий ZSet или создать новый
        let (mut dict, mut sorted) = match store.get(&key)? {
            Some(Value::ZSet { dict, sorted }) => (dict, sorted),
            Some(_) => return Err(StoreError::InvalidType),
            None => (Dict::new(), SkipList::new()),
        };

        // Сохраняем старый score (если есть) перед вставкой
        let previous_score = dict.get(&member).cloned();

        // Вставляем новый score; Dict::insert вернёт true, если элемент был новым
        let is_new = dict.insert(member.clone(), self.score);

        // Если был старый score — удаляем его из skiplist
        if let Some(old_score) = previous_score {
            sorted.remove(&OrderedFloat(old_score));
        }

        // Вставляем в skiplist новую пару (score → member)
        sorted.insert(OrderedFloat(self.score), member.clone());

        // Сохраняем обновлённый ZSet
        store.set(&key, Value::ZSet { dict, sorted })?;
        Ok(Value::Int(if is_new { 1 } else { 0 }))
    }

    fn command_name(&self) -> &'static str {
        "ZADD"
    }
}

/// Команда ZREM — удаляет элемент из упорядоченного множества.
#[derive(Debug)]
pub struct ZRemCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZRemCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        // Получаем ZSet, если он есть
        if let Some(Value::ZSet { dict, sorted }) = store.get(&key)? {
            let mut dict = dict;
            let mut sorted = sorted;

            // Сначала получаем старый score
            let old_score_opt = dict.get(&member).cloned();
            if let Some(old_score) = old_score_opt {
                // Удаляем из dict
                dict.remove(&member);
                // Удаляем из skiplist по баллу
                sorted.remove(&OrderedFloat(old_score));
                // Сохраняем обратно
                store.set(&key, Value::ZSet { dict, sorted })?;
                return Ok(Value::Int(1));
            } else {
                // Элемент не найден
                return Ok(Value::Int(0));
            }
        }

        // Ключа нет или тип не ZSet
        Ok(Value::Int(0))
    }

    fn command_name(&self) -> &'static str {
        "ZREM"
    }
}

/// Команда ZRANGE — возвращает элементы по возрастанию score.
#[derive(Debug)]
pub struct ZRangeCommand {
    pub key: String,
    pub start: i64,
    pub stop: i64,
}

impl CommandExecute for ZRangeCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::ZSet { sorted, .. }) => {
                // Собрать члены в порядке возрастания балла.
                let all: Vec<Sds> = sorted.iter().map(|(_, member)| member.clone()).collect();
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

    fn command_name(&self) -> &'static str {
        "ZRANGE"
    }
}

/// Команда ZSCORE — возвращает score для указанного элемента.
#[derive(Debug)]
pub struct ZScoreCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZScoreCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let member = Sds::from_str(&self.member);

        match store.get(&key)? {
            // Захватываем `dict` как mutable, чтобы уметь вызвать `dict.get(&member)`
            Some(Value::ZSet { mut dict, .. }) => {
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

    fn command_name(&self) -> &'static str {
        "ZSCORE"
    }
}

/// Команда ZCARD — возвращает количество элементов в упорядоченном множестве.
#[derive(Debug)]
pub struct ZCardCommand {
    pub key: String,
}

impl CommandExecute for ZCardCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        match store.get(&key)? {
            Some(Value::ZSet { dict, .. }) => Ok(Value::Int(dict.len() as i64)),
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }

    fn command_name(&self) -> &'static str {
        "ZCARD"
    }
}

/// Команда ZREVRANGE — возвращает диапазон элементов по убыванию балла.
#[derive(Debug)]
pub struct ZRevRangeCommand {
    pub key: String,
    pub start: i64,
    pub stop: i64,
}

impl CommandExecute for ZRevRangeCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);

        match store.get(&key)? {
            Some(Value::ZSet { sorted, .. }) => {
                let all: Vec<Sds> = sorted
                    .iter_rev()
                    .map(|(_, member)| member.clone())
                    .collect();
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

    fn command_name(&self) -> &'static str {
        "ZREVRANGE"
    }
}

/// Команда ZRANK — возвращает индекс элемента по возрастанию score.
#[derive(Debug)]
pub struct ZRankCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZRankCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZRANK is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZRANK"
    }
}

/// Команда ZREVRANK — возвращает индекс элемента по убыванию score.
#[derive(Debug)]
pub struct ZRevRankCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZRevRankCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZREVRANK is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZREVRANK"
    }
}

/// Команда ZCOUNT — возвращает количество элементов, score которых в диапазоне
/// [min, max].
#[derive(Debug)]
pub struct ZCountCommand {
    pub key: String,
    pub min: f64,
    pub max: f64,
}

impl CommandExecute for ZCountCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZCOUNT is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZCOUNT"
    }
}

/// Команда ZINCRBY — увеличивает score элемента на заданное значение.
#[derive(Debug)]
pub struct ZIncrByCommand {
    pub key: String,
    pub member: String,
    pub increment: f64,
}

impl CommandExecute for ZIncrByCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZINCRBY is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZINCRBY"
    }
}
