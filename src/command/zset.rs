// Copyright 2025 Zumic

use ordered_float::OrderedFloat;

use crate::{CommandExecute, Dict, QuickList, Sds, SkipList, StorageEngine, StoreError, Value};

#[derive(Debug)]
pub struct ZAddCommand {
    pub key:    String,
    pub member: String,
    pub score:  f64,
}

impl CommandExecute for ZAddCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
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

#[derive(Debug)]
pub struct ZRemCommand {
    pub key:    String,
    pub member: String,
}

impl CommandExecute for ZRemCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
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

#[derive(Debug)]
pub struct ZScoreCommand {
    pub key:    String,
    pub member: String,
}

impl CommandExecute for ZScoreCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
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

#[derive(Debug)]
pub struct ZCardCommand {
    pub key: String,
}

impl CommandExecute for ZCardCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        match store.get(&key)? {
            Some(Value::ZSet { dict, .. }) => Ok(Value::Int(dict.len() as i64)),
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Int(0)),
        }
    }
}

#[derive(Debug)]
pub struct ZRangeCommand {
    pub key:   String,
    pub start: i64,
    pub stop:  i64,
}

impl CommandExecute for ZRangeCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
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

#[derive(Debug)]
pub struct ZRevRangeCommand {
    pub key:   String,
    pub start: i64,
    pub stop:  i64,
}

impl CommandExecute for ZRevRangeCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    fn create_store() -> StorageEngine {
        StorageEngine::InMemory(InMemoryStore::new())
    }

    /// Проверяет, что добавление нового элемента в ZSet, его получение
    /// (ZSCORE) и подсчёт количества элементов (ZCARD) работают корректно.
    #[test]
    fn test_zadd_new_and_score_and_card() {
        let mut store = create_store();

        // Добавляем нового участника.
        let add = ZAddCommand {
            key:    "anton".to_string(),
            member: "a".to_string(),
            score:  1.5,
        };
        assert_eq!(add.execute(&mut store).unwrap(), Value::Int(1));

        // ZSCORE должен вернуть 1.5.
        let score = ZScoreCommand {
            key:    "anton".to_string(),
            member: "a".to_string(),
        };
        assert_eq!(score.execute(&mut store).unwrap(), Value::Float(1.5));

        // ZCARD должен вернуть 1.
        let card = ZCardCommand {
            key: "anton".to_string(),
        };
        assert_eq!(card.execute(&mut store).unwrap(), Value::Int(1));
    }

    /// Проверяет, что обновление значения в ZSet работает:
    /// при повторном добавлении участника с тем же именем возвращается 0,
    /// а значение обновляется.
    #[test]
    fn test_zadd_update_existing() {
        let mut store = create_store();
        let add1 = ZAddCommand {
            key:    "anton".into(),
            member: "a".into(),
            score:  1.0,
        };
        add1.execute(&mut store).unwrap();

        // Обновляем score для "a".
        let add2 = ZAddCommand {
            key:    "anton".into(),
            member: "a".into(),
            score:  2.0,
        };
        // Ожидаем возврат 0, так как элемент уже присутствует.
        assert_eq!(add2.execute(&mut store).unwrap(), Value::Int(0));

        // ZSCORE теперь должен вернуть 2.0.
        let score = ZScoreCommand {
            key:    "anton".into(),
            member: "a".into(),
        };
        assert_eq!(score.execute(&mut store).unwrap(), Value::Float(2.0));

        // ZCARD остаётся равным 1.
        let card = ZCardCommand {
            key: "anton".into(),
        };
        assert_eq!(card.execute(&mut store).unwrap(), Value::Int(1));
    }

    /// Проверяет удаление элемента из ZSet:
    /// после удаления (ZREM) элемент больше не доступен, а ZCARD уменьшается.
    #[test]
    fn test_zrem_and_score_and_card() {
        let mut store = create_store();
        ZAddCommand {
            key:    "anton".into(),
            member: "a".into(),
            score:  1.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key:    "anton".into(),
            member: "b".into(),
            score:  2.0,
        }
        .execute(&mut store)
        .unwrap();

        let rem = ZRemCommand {
            key:    "anton".into(),
            member: "a".into(),
        };
        assert_eq!(rem.execute(&mut store).unwrap(), Value::Int(1));

        let score_a = ZScoreCommand {
            key:    "anton".into(),
            member: "a".into(),
        };
        assert_eq!(score_a.execute(&mut store).unwrap(), Value::Null);

        let card = ZCardCommand {
            key: "anton".into(),
        };
        assert_eq!(card.execute(&mut store).unwrap(), Value::Int(1));

        let rem2 = ZRemCommand {
            key:    "anton".into(),
            member: "c".into(),
        };
        assert_eq!(rem2.execute(&mut store).unwrap(), Value::Int(0));
    }

    /// Проверяет работу команды ZRANGE с положительными и отрицательными
    /// индексами: выбирает элементы в полном диапазоне, а также с заданным диапазоном.
    #[test]
    fn test_zrange_basic_and_negative() {
        let mut store = create_store();
        // Добавляем элементы: a:1, b:2, c:3.
        ZAddCommand {
            key:    "anton".into(),
            member: "a".into(),
            score:  1.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key:    "anton".into(),
            member: "b".into(),
            score:  2.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key:    "anton".into(),
            member: "c".into(),
            score:  3.0,
        }
        .execute(&mut store)
        .unwrap();

        // полный диапазон
        let zr = ZRangeCommand {
            key:   "anton".into(),
            start: 0,
            stop:  -1,
        };
        let list = match zr.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list,
            vec![Sds::from_str("a"), Sds::from_str("b"), Sds::from_str("c"),]
        );

        let zr2 = ZRangeCommand {
            key:   "anton".into(),
            start: 1,
            stop:  2,
        };
        let list2 = match zr2.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(list2, vec![Sds::from_str("b"), Sds::from_str("c"),]);

        // отрицательные индексы: последние два
        let zr3 = ZRangeCommand {
            key:   "anton".into(),
            start: -2,
            stop:  -1,
        };
        let list3 = match zr3.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(list3, vec![Sds::from_str("b"), Sds::from_str("c"),]);
    }

    /// Проверяет работу команды ZREVRANGE (обратный порядок):
    /// выбирает элементы в обратном порядке как для полного диапазона,
    /// так и для заданного диапазона индексов.
    #[test]
    fn test_zrevrange_basic_and_negative() {
        let mut store = create_store();
        // a:1, b:2, c:3
        ZAddCommand {
            key:    "anton".into(),
            member: "a".into(),
            score:  1.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key:    "anton".into(),
            member: "b".into(),
            score:  2.0,
        }
        .execute(&mut store)
        .unwrap();
        ZAddCommand {
            key:    "anton".into(),
            member: "c".into(),
            score:  3.0,
        }
        .execute(&mut store)
        .unwrap();

        // полный диапазон
        let zr = ZRevRangeCommand {
            key:   "anton".into(),
            start: 0,
            stop:  -1,
        };
        let list = match zr.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(
            list,
            vec![Sds::from_str("c"), Sds::from_str("b"), Sds::from_str("a"),]
        );

        // отрицательные индексы: первые два в реверсе
        let zr2 = ZRevRangeCommand {
            key:   "anton".into(),
            start: 0,
            stop:  1,
        };
        let list2 = match zr2.execute(&mut store).unwrap() {
            Value::List(l) => l.into_iter().collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(list2, vec![Sds::from_str("c"), Sds::from_str("b"),]);
    }

    /// Проверяет команды ZRANGE, ZREVRANGE, ZSCORE и ZREM на несуществующем ключе:
    /// в данном случае должны возвращаться Value::Null и Value::Int(0).
    #[test]
    fn test_zcommands_on_nonexistent_key() {
        let mut store = create_store();
        // на несуществующем ключе ZRANGE и ZREVRANGE возвращают Null
        let zr = ZRangeCommand {
            key:   "no".into(),
            start: 0,
            stop:  -1,
        };
        assert_eq!(zr.execute(&mut store).unwrap(), Value::Null);

        let zr2 = ZRevRangeCommand {
            key:   "no".into(),
            start: 0,
            stop:  -1,
        };
        assert_eq!(zr2.execute(&mut store).unwrap(), Value::Null);

        // ZSCORE и ZREM на несуществующем — Null и Int(0)
        let zs = ZScoreCommand {
            key:    "no".into(),
            member: "m".into(),
        };
        assert_eq!(zs.execute(&mut store).unwrap(), Value::Null);

        let zr = ZRemCommand {
            key:    "no".into(),
            member: "m".into(),
        };
        assert_eq!(zr.execute(&mut store).unwrap(), Value::Int(0));
    }
}
