use ordered_float::OrderedFloat;
use std::collections::{BTreeMap, LinkedList, VecDeque};

use super::{CommandError, CommandExecute};
use crate::{ArcBytes, QuickList, StorageEntry, Store, Value};

/// `ZAddCommand` — добавляет элемент с определённым баллом в отсортированное множество.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `score`: балл, с которым элемент добавляется.
/// - `member`: элемент, добавляемый в множество.
#[derive(Debug)]
pub struct ZAddCommand {
    pub key: String,
    pub score: f64,
    pub member: String,
}

impl CommandExecute for ZAddCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        let mut entry = store
            .data
            .entry(self.key.clone())
            .or_insert_with(|| StorageEntry {
                value: Value::ZSet(BTreeMap::new()),
            });
        if let Value::ZSet(ref mut zset) = entry.value {
            let added = zset
                .insert(OrderedFloat(self.score), self.member.clone())
                .is_none();
            return Ok(Value::Int(if added { 1 } else { 0 }));
        }
        Err(CommandError::WrongType)
    }
}

/// `ZRemCommand` — удаляет элемент из отсортированного множества.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `member`: элемент, который нужно удалить.
#[derive(Debug)]
pub struct ZRemCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZRemCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        if let Some(mut entry) = store.data.get_mut(&self.key) {
            if let Value::ZSet(ref mut zset) = entry.value {
                let original_len = zset.len();
                zset.retain(|_, v| v != &self.member);
                let removed = (original_len != zset.len()) as i64;
                return Ok(Value::Int(removed));
            }
            return Err(CommandError::WrongType);
        }
        Err(CommandError::KeyNotFound)
    }
}

/// `ZRankCommand` — возвращает ранг элемента в отсортированном множестве.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `member`: элемент для поиска ранга.
#[derive(Debug)]
pub struct ZRankCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZRankCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        if let Some(entry) = store.data.get(&self.key) {
            if let Value::ZSet(zset) = &entry.value {
                for (rank, (_, v)) in zset.iter().enumerate() {
                    if v == &self.member {
                        return Ok(Value::Int(rank as i64));
                    }
                }
                return Err(CommandError::KeyNotFound);
            }
            return Err(CommandError::WrongType);
        }
        Err(CommandError::KeyNotFound)
    }
}

/// `ZScoreCommand` — возвращает балл элемента в отсортированном множестве.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `member`: элемент для поиска его балла.
#[derive(Debug)]
pub struct ZScoreCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZScoreCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        if let Some(entry) = store.data.get(&self.key) {
            if let Value::ZSet(zset) = &entry.value {
                for (score, v) in zset.iter() {
                    if v == &self.member {
                        return Ok(Value::Float(score.0));
                    }
                }
                return Err(CommandError::KeyNotFound);
            }
            return Err(CommandError::WrongType);
        }
        Err(CommandError::KeyNotFound)
    }
}

/// `ZCardCommand` — возвращает количество элементов в отсортированном множестве.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
#[derive(Debug)]
pub struct ZCardCommand {
    pub key: String,
}

impl CommandExecute for ZCardCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        if let Some(entry) = store.data.get(&self.key) {
            if let Value::ZSet(zset) = &entry.value {
                return Ok(Value::Int(zset.len() as i64));
            }
            return Err(CommandError::WrongType);
        }
        Err(CommandError::KeyNotFound)
    }
}

/// `ZRangeCommand` — возвращает элементы в заданном диапазоне индексов.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `start`: начальный индекс диапазона.
/// - `stop`: конечный индекс диапазона.
#[derive(Debug)]
pub struct ZRangeCommand {
    pub key: String,
    pub start: i64,
    pub stop: i64,
    pub scores: bool,
}

impl CommandExecute for ZRangeCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        let entry = store.data.get(&self.key).ok_or(CommandError::KeyNotFound)?;

        if let Value::ZSet(zset) = &entry.value {
            let len = zset.len() as i64;
            let (start, stop) = StorageEntry::normalize_range(self.start, self.stop, len);

            let mut result = VecDeque::new();
            for (score, member) in zset
                .iter()
                .skip(start as usize)
                .take((stop - start + 1) as usize)
            {
                result.push_back(Value::Str(ArcBytes::from_bytes(member.as_bytes())));
                if self.scores {
                    result.push_back(Value::Float(score.0));
                }
            }

            Ok(Value::List(LinkedList::from([result])))
            Ok(Value::List(QuickList::))
        } else {
            Err(CommandError::WrongType)
        }
    }
}

/// `ZRevRankCommand` — возвращает ранг элемента (сортировка по убыванию баллов).
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `member`: элемент для поиска ранга.
#[derive(Debug)]
pub struct ZRevRankCommand {
    pub key: String,
    pub member: String,
}

impl CommandExecute for ZRevRankCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        let entry = store.data.get(&self.key).ok_or(CommandError::KeyNotFound)?;

        if let Value::ZSet(zset) = &entry.value {
            let total = zset.len() as i64;
            let forward_rank = zset
                .values()
                .position(|m| m == &self.member)
                .ok_or(CommandError::KeyNotFound)? as i64;

            Ok(Value::Int(total - 1 - forward_rank))
        } else {
            Err(CommandError::WrongType)
        }
    }
}

/// `ZCountCommand` — возвращает подсчитанные элементы с баллами в заданном диапазоне.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `min`: минимальный балл.
/// - `max`: максимальный балл.
#[derive(Debug)]
pub struct ZCountCommand {
    pub key: String,
    pub min: f64,
    pub max: f64,
}

impl CommandExecute for ZCountCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        let entry = store.data.get(&self.key).ok_or(CommandError::KeyNotFound)?;

        if let Value::ZSet(zset) = &entry.value {
            let count = zset
                .range(OrderedFloat(self.min)..=OrderedFloat(self.max))
                .count() as i64;

            Ok(Value::Int(count))
        } else {
            Err(CommandError::WrongType)
        }
    }
}

/// `ZIncrByCommand` — увеличивает балл элемента в отсортированном множестве на заданное значение.
///
/// # Поля:
/// - `key`: ключ для отсортированного множества.
/// - `increment`: значение, на которое нужно увеличить балл.
/// - `member`: элемент, чьё значение нужно увеличить.
#[derive(Debug)]
pub struct ZIncrByCommand {
    pub key: String,
    pub increment: f64,
    pub member: String,
}

impl CommandExecute for ZIncrByCommand {
    fn execute(&self, store: &Store) -> Result<Value, CommandError> {
        let mut entry = store
            .data
            .entry(self.key.clone())
            .or_insert_with(|| StorageEntry {
                value: Value::ZSet(BTreeMap::new()),
            });

        if let Value::ZSet(ref mut zset) = entry.value {
            // Находим текущий балл
            let current_score = zset
                .iter()
                .find(|(_, m)| *m == &self.member)
                .map(|(s, _)| s.0)
                .unwrap_or(0.0);

            // Удаляем старые записи
            zset.retain(|_, m| m != &self.member);

            // Добавляем новую запись
            let new_score = current_score + self.increment;
            zset.insert(OrderedFloat(new_score), self.member.clone());

            Ok(Value::Float(new_score))
        } else {
            Err(CommandError::WrongType)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArcBytes, CommandExecute, Store, Value};

    /// Тестирование команды `ZAddCommand` — добавление элементов в отсортированное множество,
    /// и команды `ZCardCommand` — подсчёт количества элементов в этом множестве.
    #[test]
    fn test_zadd_zcard() {
        let store: Store = Store::new();

        let zadd1 = ZAddCommand {
            key: "my_zset".to_string(),
            score: 10.5,
            member: "Anton".to_string(),
        };

        let zadd2 = ZAddCommand {
            key: "my_zset".to_string(),
            score: 20.0,
            member: "Boris".to_string(),
        };

        let zadd3 = ZAddCommand {
            key: "my_zset".to_string(),
            score: 15.0,
            member: "Elena".to_string(),
        };

        assert_eq!(zadd1.execute(&store), Ok(Value::Int(1)));
        assert_eq!(zadd2.execute(&store), Ok(Value::Int(1)));
        assert_eq!(zadd3.execute(&store), Ok(Value::Int(1)));

        let zcard = ZCardCommand {
            key: "my_zset".to_string(),
        };

        assert_eq!(zcard.execute(&store), Ok(Value::Int(3)));
    }

    /// Тестирование команды `ZRemCommand` — удаление элемента из отсортированного множества.
    #[test]
    fn test_zrem() {
        let store: Store = Store::new();

        let zadd = ZAddCommand {
            key: "my_zset".to_string(),
            score: 10.5,
            member: "Anton".to_string(),
        };
        zadd.execute(&store).unwrap();

        let zrem = ZRemCommand {
            key: "my_zset".to_string(),
            member: "Anton".to_string(),
        };

        assert_eq!(zrem.execute(&store), Ok(Value::Int(1)));

        // Проверяем повторённое удаление.
        assert_eq!(zrem.execute(&store), Ok(Value::Int(0)));
    }

    /// Тестирование команды `ZRankCommand` — получение ранга элемента в отсортированном множестве.
    #[test]
    fn test_zrank() {
        let store: Store = Store::new();

        let zadd1 = ZAddCommand {
            key: "my_zset".to_string(),
            score: 10.5,
            member: "Anton".to_string(),
        };

        let zadd2 = ZAddCommand {
            key: "my_zset".to_string(),
            score: 20.0,
            member: "Boris".to_string(),
        };

        let zadd3 = ZAddCommand {
            key: "my_zset".to_string(),
            score: 15.0,
            member: "Elena".to_string(),
        };

        zadd1.execute(&store).unwrap();
        zadd2.execute(&store).unwrap();
        zadd3.execute(&store).unwrap();

        let zrank = ZRankCommand {
            key: "my_zset".to_string(),
            member: "Anton".to_string(),
        };
        assert_eq!(zrank.execute(&store), Ok(Value::Int(0)));

        let zrank_boris = ZRankCommand {
            key: "my_zset".to_string(),
            member: "Boris".to_string(),
        };
        assert_eq!(zrank_boris.execute(&store), Ok(Value::Int(2)));
    }

    /// Тестирование команды `ZScoreCommand` — получение балла элемента из отсортированного множества.
    #[test]
    fn test_zscore() {
        let store: Store = Store::new();

        let zadd = ZAddCommand {
            key: "my_zset".to_string(),
            score: 42.0,
            member: "Eva".to_string(),
        };
        zadd.execute(&store).unwrap();

        let zscore = ZScoreCommand {
            key: "my_zset".to_string(),
            member: "Eva".to_string(),
        };
        assert_eq!(zscore.execute(&store), Ok(Value::Float(42.0)));
    }

    /// Тестирование команды `ZScoreCommand` — попытка получения балла для несуществующего элемента.
    #[test]
    fn test_zscore_not_found() {
        let store: Store = Store::new();

        let zscore = ZScoreCommand {
            key: "my_zset".to_string(),
            member: "Uknown".to_string(),
        };
        assert_eq!(zscore.execute(&store), Err(CommandError::KeyNotFound));
    }

    /// Тестирование команды `ZRangeCommand` — получение элементов из отсортированного множества по индексу.
    #[test]
    fn test_zrange() {
        let store = Store::new();

        ZAddCommand {
            key: "my_zset".to_string(),
            score: 1.0,
            member: "a".into(),
        }
        .execute(&store)
        .unwrap();
        ZAddCommand {
            key: "my_zset".to_string(),
            score: 2.0,
            member: "b".into(),
        }
        .execute(&store)
        .unwrap();

        let zrange_cmd = ZRangeCommand {
            key: "my_zset".to_string(),
            start: 0,
            stop: 1,
            scores: false,
        };
        if let Ok(Value::List(list)) = zrange_cmd.execute(&store) {
            let expected = vec![
                Value::Str(ArcBytes::from_bytes(b"a")),
                Value::Str(ArcBytes::from_bytes(b"b")),
            ];
            assert_eq!(list.front().unwrap().clone(), VecDeque::from(expected));
        }
    }

    /// Тестирование команды `ZRevRankCommand` — получение позиции элемента в отсортированном множестве по убыванию баллов.
    #[test]
    fn test_zrevrank() {
        let store = Store::new();

        ZAddCommand {
            key: "my_zset".to_string(),
            score: 1.0,
            member: "a".into(),
        }
        .execute(&store)
        .unwrap();
        ZAddCommand {
            key: "my_zset".to_string(),
            score: 2.0,
            member: "b".into(),
        }
        .execute(&store)
        .unwrap();

        let zrevrank_cmd = ZRevRankCommand {
            key: "my_zset".to_string(),
            member: "a".into(),
        };
        assert_eq!(zrevrank_cmd.execute(&store), Ok(Value::Int(1)));
    }

    /// Тестирование команды `ZCountCommand` — подсчет элементов в отсортированном множестве по диапазону баллов.
    #[test]
    fn test_zcount() {
        let store = Store::new();

        ZAddCommand {
            key: "my_zset".to_string(),
            score: 1.0,
            member: "a".into(),
        }
        .execute(&store)
        .unwrap();
        ZAddCommand {
            key: "my_zset".to_string(),
            score: 2.0,
            member: "b".into(),
        }
        .execute(&store)
        .unwrap();

        let zcount_cmd = ZCountCommand {
            key: "my_zset".to_string(),
            min: 1.5,
            max: 2.5,
        };
        assert_eq!(zcount_cmd.execute(&store), Ok(Value::Int(1)));
    }

    /// Тестирование команды `ZIncrByCommand` — увеличение балла элемента в отсортированном множестве.
    #[test]
    fn test_zincrby() {
        let store = Store::new();

        ZAddCommand {
            key: "my_zset".to_string(),
            score: 1.0,
            member: "a".into(),
        }
        .execute(&store)
        .unwrap();

        let zincrby_cmd = ZIncrByCommand {
            key: "my_zset".to_string(),
            member: "a".into(),
            increment: 2.5,
        };

        assert_eq!(zincrby_cmd.execute(&store), Ok(Value::Float(3.5)));

        let score_cmd = ZScoreCommand {
            key: "my_zset".to_string(),
            member: "a".into(),
        };
        assert_eq!(score_cmd.execute(&store), Ok(Value::Float(3.5)));
    }
}
