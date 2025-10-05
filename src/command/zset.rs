//! Команды для работы с упорядоченными множествами (ZSet, sorted set) в Zumic.
//!
//! Реализует команды ZADD, ZREM, ZSCORE, ZCARD, ZRANGE, ZREVRANGE для
//! управления элементами с баллами (score). Каждая команда реализует трейт
//! [`CommandExecute`].

use ordered_float::OrderedFloat;

use crate::{CommandExecute, Dict, QuickList, Sds, SkipList, StorageEngine, StoreError, Value};

/// Команда ZADD — добавляет элемент с баллом (score) в упорядоченное множество.
///
/// Формат: `ZADD key score member`
///
/// # Поля
/// * `key` — ключ множества.
/// * `member` — добавляемый элемент.
/// * `score` — балл (score) элемента.
///
/// # Возвращает
/// 1, если элемент был добавлен, 0 — если обновлён.
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
}

/// Команда ZREM — удаляет элемент из упорядоченного множества.
///
/// Формат: `ZREM key member`
///
/// # Поля
/// * `key` — ключ множества.
/// * `member` — удаляемый элемент.
///
/// # Возвращает
/// 1, если элемент был удалён, 0 — если не найден.
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
}

/// Команда ZSCORE — возвращает балл (score) элемента.
///
/// Формат: `ZSCORE key member`
///
/// # Поля
/// * `key` — ключ множества.
/// * `member` — элемент.
///
/// # Возвращает
/// Балл (score) элемента или `Null`, если элемент не найден.
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
}

/// Команда ZCARD — возвращает количество элементов в упорядоченном множестве.
///
/// Формат: `ZCARD key`
///
/// # Поля
/// * `key` — ключ множества.
///
/// # Возвращает
/// Количество элементов в множестве.
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
}

/// Команда ZRANGE — возвращает диапазон элементов по возрастанию балла.
///
/// Формат: `ZRANGE key start stop`
///
/// # Поля
/// * `key` — ключ множества.
/// * `start` — начальный индекс.
/// * `stop` — конечный индекс.
///
/// # Возвращает
/// Список элементов в заданном диапазоне или `Null`, если множество не
/// существует.
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
}

/// Команда ZREVRANGE — возвращает диапазон элементов по убыванию балла.
///
/// Формат: `ZREVRANGE key start stop`
///
/// # Поля
/// * `key` — ключ множества.
/// * `start` — начальный индекс.
/// * `stop` — конечный индекс.
///
/// # Возвращает
/// Список элементов в заданном диапазоне или `Null`, если множество не
/// существует.
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
                // Собрать члены в обратном порядке по баллу.
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
}
