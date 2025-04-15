use std::collections::HashMap;

use ordered_float::OrderedFloat;

use crate::{
    database::{arcbytes::ArcBytes, quicklist::QuickList, skip_list::SkipList, types::Value},
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
            None => (HashMap::new(), SkipList::new()),
        };

        // Вставляем новый score, возвращая старое значение, если оно было
        let previous = dict.insert(member.clone(), self.score);
        let is_new = previous.is_none();

        // Если член уже присутствовал, удаляем старую запись в skiplist с его старым score.
        if let Some(old_score) = previous {
            sorted.remove(&OrderedFloat(old_score));
        }

        // Вставляем новую запись: ключом является OrderedFloat(score), а значением — member.
        sorted.insert(OrderedFloat(self.score), member.clone());

        // Сохраняем обратно обновлённый ZSet
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

        if let Some(Value::ZSet { dict, sorted }) = store.get(key.clone())? {
            let mut dict = dict;
            let mut sorted = sorted;
            if let Some(old_score) = dict.remove(&member) {
                // Удаляем из skiplist по score
                sorted.remove(&OrderedFloat(old_score));
                store.set(key, Value::ZSet { dict, sorted })?;
                return Ok(Value::Int(1));
            }
            // Элемент не найден
            return Ok(Value::Int(0));
        }
        // Нет такого ключа или тип не соответствует
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
                // Собираем членов в порядке возрастания score
                let all: Vec<ArcBytes> = sorted.iter().map(|(_, member)| member.clone()).collect();
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
                // Собираем членов в обратном порядке по score
                let all: Vec<ArcBytes> = sorted
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

#[cfg(test)]
mod tests {
    use crate::engine::memory::InMemoryStore;

    use super::*;

    fn create_store() -> StorageEngine {
        StorageEngine::InMemory(InMemoryStore::new())
    }

    #[test]
    fn test_zadd_new_and_score_and_card() {
        let mut store = create_store();

        // Добавляем нового members
        let add = ZAddCommand {
            key: "anton".to_string(),
            member: "a".to_string(),
            score: 1.5,
        };
        assert_eq!(add.execute(&mut store).unwrap(), Value::Int(1));

        // ZSCORE должен вернуть 1.5
        let score = ZScoreCommand {
            key: "anton".to_string(),
            member: "a".to_string(),
        };
        assert_eq!(score.execute(&mut store).unwrap(), Value::Float(1.5));

        // ZCARD должен быть 1
        let card = ZCardCommand {
            key: "anton".to_string(),
        };
        assert_eq!(card.execute(&mut store).unwrap(), Value::Int(1));
    }

    #[test]
    fn test_zadd_update_existing() {
        let mut store = create_store();
        let add1 = ZAddCommand {
            key: "anton".into(),
            member: "a".into(),
            score: 1.0,
        };
        add1.execute(&mut store).unwrap();

        // Обновляем score у "a"
        let add2 = ZAddCommand {
            key: "anton".into(),
            member: "a".into(),
            score: 2.0,
        };
        // должен вернуть 0 — элемент не новый
        assert_eq!(add2.execute(&mut store).unwrap(), Value::Int(0));

        // ZSCORE теперь 2.0
        let score = ZScoreCommand {
            key: "anton".into(),
            member: "a".into(),
        };
        assert_eq!(score.execute(&mut store).unwrap(), Value::Float(2.0));

        // ZCARD по‑прежнему 1
        let card = ZCardCommand {
            key: "anton".into(),
        };
        assert_eq!(card.execute(&mut store).unwrap(), Value::Int(1));
    }

    #[test]
    fn test_zrem_and_score_and_card() {
        let mut store = create_store();
        // подготовка
        ZAddCommand {
            key: "anton".into(),
            member: "a".into(),
            score: 1.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key: "anton".into(),
            member: "b".into(),
            score: 2.0,
        }
        .execute(&mut store)
        .unwrap();

        // удаляем "a"
        let rem = ZRemCommand {
            key: "anton".into(),
            member: "a".into(),
        };
        assert_eq!(rem.execute(&mut store).unwrap(), Value::Int(1));

        // "a" больше нет
        let score_a = ZScoreCommand {
            key: "anton".into(),
            member: "a".into(),
        };
        assert_eq!(score_a.execute(&mut store).unwrap(), Value::Null);

        // ZCARD теперь 1
        let card = ZCardCommand {
            key: "anton".into(),
        };
        assert_eq!(card.execute(&mut store).unwrap(), Value::Int(1));

        // удаление несуществующего
        let rem2 = ZRemCommand {
            key: "anton".into(),
            member: "c".into(),
        };
        assert_eq!(rem2.execute(&mut store).unwrap(), Value::Int(0));
    }

    #[test]
    fn test_zrange_basic_and_negative() {
        let mut store = create_store();
        // a:1, b:2, c:3
        ZAddCommand {
            key: "anton".into(),
            member: "a".into(),
            score: 1.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key: "anton".into(),
            member: "b".into(),
            score: 2.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key: "anton".into(),
            member: "c".into(),
            score: 3.0,
        }
        .execute(&mut store)
        .unwrap();

        // полный диапазон
        let zr = ZRangeCommand {
            key: "anton".into(),
            start: 0,
            stop: -1,
        };
        let list = match zr.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list,
            vec![
                ArcBytes::from_str("a"),
                ArcBytes::from_str("b"),
                ArcBytes::from_str("c"),
            ]
        );

        // частичный
        let zr2 = ZRangeCommand {
            key: "anton".into(),
            start: 1,
            stop: 2,
        };
        let list2 = match zr2.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list2,
            vec![ArcBytes::from_str("b"), ArcBytes::from_str("c"),]
        );

        // отрицательные индексы: последние два
        let zr3 = ZRangeCommand {
            key: "anton".into(),
            start: -2,
            stop: -1,
        };
        let list3 = match zr3.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list3,
            vec![ArcBytes::from_str("b"), ArcBytes::from_str("c"),]
        );
    }

    #[test]
    fn test_zrevrange_basic_and_negative() {
        let mut store = create_store();
        // a:1, b:2, c:3
        ZAddCommand {
            key: "anton".into(),
            member: "a".into(),
            score: 1.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key: "anton".into(),
            member: "b".into(),
            score: 2.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key: "anton".into(),
            member: "c".into(),
            score: 3.0,
        }
        .execute(&mut store)
        .unwrap();

        // полный диапазон
        let zr = ZRevRangeCommand {
            key: "anton".into(),
            start: 0,
            stop: -1,
        };
        let list = match zr.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list,
            vec![
                ArcBytes::from_str("c"),
                ArcBytes::from_str("b"),
                ArcBytes::from_str("a"),
            ]
        );

        // отрицательные индексы: первые два в реверсе
        let zr2 = ZRevRangeCommand {
            key: "anton".into(),
            start: 0,
            stop: 1,
        };
        let list2 = match zr2.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list2,
            vec![ArcBytes::from_str("c"), ArcBytes::from_str("b"),]
        );
    }

    #[test]
    fn test_zcommands_on_nonexistent_key() {
        let mut store = create_store();
        // на несуществующем ключе ZRANGE и ZREVRANGE возвращают Null
        let zr = ZRangeCommand {
            key: "no".into(),
            start: 0,
            stop: -1,
        };
        assert_eq!(zr.execute(&mut store).unwrap(), Value::Null);

        let zr2 = ZRevRangeCommand {
            key: "no".into(),
            start: 0,
            stop: -1,
        };
        assert_eq!(zr2.execute(&mut store).unwrap(), Value::Null);

        // ZSCORE и ZREM на несуществующем — Null и Int(0)
        let zs = ZScoreCommand {
            key: "no".into(),
            member: "m".into(),
        };
        assert_eq!(zs.execute(&mut store).unwrap(), Value::Null);

        let zr = ZRemCommand {
            key: "no".into(),
            member: "m".into(),
        };
        assert_eq!(zr.execute(&mut store).unwrap(), Value::Int(0));
    }
}
