use crate::{CommandExecute, StorageEngine, StoreError, Value};

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
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZADD is not implemented yet");
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
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZREM is not implemented yet");
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
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZRANGE is not implemented yet");
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
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZSCORE is not implemented yet");
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
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZCARD is not implemented yet");
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
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZREVRANGE is not implemented yet");
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

/// Команда ZRANGEBYSCORE — возвращает элементы с score в диапазоне [min, max].
#[derive(Debug)]
pub struct ZRangeByScoreCommand {
    pub key: String,
    pub min: f64,
    pub max: f64,
}

impl CommandExecute for ZRangeByScoreCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZRANGEBYSCORE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZRANGEBYSCORE"
    }
}

/// Команда ZRANGEBYLEX — возвращает элементы в лексикографическом диапазоне.
#[derive(Debug)]
pub struct ZRangeByLexCommand {
    pub key: String,
    pub min: String,
    pub max: String,
}

impl CommandExecute for ZRangeByLexCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZRANGEBYLEX is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZRANGEBYLEX"
    }
}

/// Команда ZUNIONSTORE — объединяет несколько ZSET и сохраняет результат в
/// dest.
#[derive(Debug)]
pub struct ZUnionStoreCommand {
    pub destination: String,
    pub keys: Vec<String>,
}

impl CommandExecute for ZUnionStoreCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZUNIONSTORE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZUNIONSTORE"
    }
}

/// Команда ZINTERSTORE — пересечение нескольких ZSET.
#[derive(Debug)]
pub struct ZInterStoreCommand {
    pub destination: String,
    pub keys: Vec<String>,
}

impl CommandExecute for ZInterStoreCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZINTERSTORE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZINTERSTORE"
    }
}

/// Команда ZPOPMIN — удаляет и возвращает элемент с минимальным score.
#[derive(Debug)]
pub struct ZPopMinCommand {
    pub key: String,
    pub count: Option<usize>,
}

impl CommandExecute for ZPopMinCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZPOPMIN is not implemented yet");
    }

    fn command_name(&self) -> &'static str {
        "ZPOPMIN"
    }
}

/// Команда ZPOPMAX — удаляет и возвращает элемент с максимальным score.
#[derive(Debug)]
pub struct ZPopMaxCommand {
    pub key: String,
    pub count: Option<usize>,
}

impl CommandExecute for ZPopMaxCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("ZPOPMAX is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "ZPOPMAX"
    }
}
