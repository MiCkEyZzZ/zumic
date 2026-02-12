//! Абстракция и диспетчеризация команд Zumic.
//!
//! Содержит трейт [`CommandExecute`] для унифицированного выполнения команд и
//! enum [`Command`], инкапсулирующий все поддерживаемые команды. Это позволяет
//! обрабатывать любые команды через единый интерфейс.

use std::time::Instant;

use super::{
    pubsub::{PSubscribeCommand, PUnsubscribeCommand},
    AppendCommand, AuthCommand, BitCountCommand, BitOpCommand, BitPosCommand, DecrByCommand,
    DecrCommand, DelCommand, ExistsCommand, FlushDbCommand, GeoAddCommand, GeoPosCommand,
    GeoRadiusByMemberCommand, GeoRadiusCommand, GetBitCommand, GetCommand, GetDistCommand,
    GetRangeCommand, HDelCommand, HExistsCommand, HGetAllCommand, HGetCommand, HKeysCommand,
    HLenCommand, HRandFieldCommand, HSetCommand, HValsCommand, HmGetCommand, IncrByCommand,
    IncrByFloatCommand, IncrCommand, LLenCommand, LPopCommand, LPushCommand, LRangeCommand,
    LRemCommand, LSetCommand, MGetCommand, MSetCommand, PfAddCommand, PfCountCommand,
    PfMergeCommand, RPopCommand, RPushCommand, RenameCommand, RenameNxCommand, SAddCommand,
    SCardCommand, SDiffCommand, SInterCommand, SIsMemberCommand, SMembersCommand, SPopCommand,
    SRandMemberCommand, SRemCommand, SUnionCommand, SetBitCommand, SetCommand, SetFloatCommand,
    SetNxCommand, StrLenCommand, XAckCommand, XAddCommand, XDelCommand, XGroupCreateCommand,
    XLenCommand, XRangeCommand, XReadCommand, XRevRangeCommand, XTrimCommand, ZAddCommand,
    ZCardCommand, ZCountCommand, ZIncrByCommand, ZRangeCommand, ZRankCommand, ZRemCommand,
    ZRevRangeCommand, ZRevRankCommand, ZScoreCommand,
};
use crate::{
    command::{
        pubsub::{PubSubCommand, PublishCommand, SubscribeCommand, UnsubscribeCommand},
        BgSaveCommand, DbSizeCommand, EchoCommand, InfoCommand, PingCommand, SaveCommand,
        SelectCommand, ShutdownCommand, TimeCommand,
    },
    logging::slow_log::SlowQueryTracker,
    StorageEngine, StoreError, Value,
};

/// Обёртка для выполнения команды с дополнительным контекстом.
pub struct CommandExecutor {
    pub client_addr: Option<String>,
    pub slot_id: Option<u16>,
}

pub trait CommandExecute: std::fmt::Debug {
    /// Выполняет команду, взаимодействуя с хранилищем.
    ///
    /// Метод изменяет состояние хранилища (если команда подразумевает
    /// изменения) и возвращает результат выполнения.
    ///
    /// # Параметры
    /// - `store` — ссылка на хранилище, над которым выполняется команда.
    ///
    /// # Возвращает
    /// - `Ok(Value)` — результат выполнения команды (например, полученное
    ///   значение, количество, статус и т.д.).
    /// - `Err(StoreError)` — если произошла ошибка при выполнении (например,
    ///   неверный тип, отсутствие ключа, сбой хранилища).
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError>;

    /// Возвращает имя команды для logging
    fn command_name(&self) -> &'static str {
        "UNKNOWN"
    }

    /// Возвращает ключ команды (если есть) для logging
    fn command_key(&self) -> Option<&[u8]> {
        None
    }
}

/// Перечисление всех поддерживаемых команд Zumic.
///
/// Каждый вариант содержит структуру соответствующей команды.
/// Enum реализует [`CommandExecute`], что позволяет выполнять любую команду
/// через единый интерфейс.
///
/// Обычно используется для парсинга и диспетчеризации команд, полученных от
/// клиента.
#[derive(Debug)]
pub enum Command {
    Set(SetCommand),
    Get(GetCommand),
    SetNx(SetNxCommand),
    MSet(MSetCommand),
    MGet(MGetCommand),
    Strlen(StrLenCommand),
    Append(AppendCommand),
    GetRange(GetRangeCommand),
    Del(DelCommand),
    Exists(ExistsCommand),
    Rename(RenameCommand),
    Renamenx(RenameNxCommand),
    Flushdb(FlushDbCommand),
    Incr(IncrCommand),
    Incrby(IncrByCommand),
    Decr(DecrCommand),
    Decrby(DecrByCommand),
    Incrbyfloat(IncrByFloatCommand),
    Decrbyfloat(DecrByCommand),
    Setfloat(SetFloatCommand),
    HSet(HSetCommand),
    HGet(HGetCommand),
    HmGet(HmGetCommand),
    HDel(HDelCommand),
    HExists(HExistsCommand),
    HLen(HLenCommand),
    HKeys(HKeysCommand),
    HVals(HValsCommand),
    HGetall(HGetAllCommand),
    HRandField(HRandFieldCommand),
    HPfAdd(PfAddCommand),
    HPfCount(PfCountCommand),
    HPfMerge(PfMergeCommand),
    SAdd(SAddCommand),
    SRem(SRemCommand),
    SCard(SCardCommand),
    SMembers(SMembersCommand),
    SIsmember(SIsMemberCommand),
    SRandMember(SRandMemberCommand),
    SPop(SPopCommand),
    SInter(SInterCommand),
    SUnion(SUnionCommand),
    SDiff(SDiffCommand),
    ZAdd(ZAddCommand),
    ZRem(ZRemCommand),
    ZRange(ZRangeCommand),
    ZScore(ZScoreCommand),
    ZCard(ZCardCommand),
    ZRevrange(ZRevRangeCommand),
    ZRank(ZRankCommand),
    ZRevRank(ZRevRankCommand),
    ZCount(ZCountCommand),
    ZIncrBy(ZIncrByCommand),
    LSet(LSetCommand),
    LRem(LRemCommand),
    LPush(LPushCommand),
    RPush(RPushCommand),
    LPop(LPopCommand),
    RPop(RPopCommand),
    LLen(LLenCommand),
    LRange(LRangeCommand),
    Auth(AuthCommand),
    GeoAdd(GeoAddCommand),
    GeoDist(GetDistCommand),
    GeoPos(GeoPosCommand),
    GeoRadius(GeoRadiusCommand),
    GeoRadiusByMember(GeoRadiusByMemberCommand),
    SetBit(SetBitCommand),
    GetBit(GetBitCommand),
    BitCount(BitCountCommand),
    BitOp(BitOpCommand),
    BitPos(BitPosCommand),
    Subscribe(SubscribeCommand),
    Unsubscribe(UnsubscribeCommand),
    Publish(PublishCommand),
    PSubscribe(PSubscribeCommand),
    PUnsubscribe(PUnsubscribeCommand),
    PubSub(PubSubCommand),
    Ping(PingCommand),
    Echo(EchoCommand),
    DbSize(DbSizeCommand),
    Info(InfoCommand),
    Time(TimeCommand),
    Select(SelectCommand),
    Save(SaveCommand),
    BgSave(BgSaveCommand),
    Shutdown(ShutdownCommand),
    XAdd(XAddCommand),
    XRead(XReadCommand),
    XRange(XRangeCommand),
    XRevRange(XRevRangeCommand),
    XLen(XLenCommand),
    XDel(XDelCommand),
    XTrim(XTrimCommand),
    XGroupCreate(XGroupCreateCommand),
    XAck(XAckCommand),
}

impl Command {
    /// Возвращает имя команды.
    pub fn name(&self) -> &'static str {
        match self {
            Command::Set(_) => "SET",
            Command::Get(_) => "GET",
            Command::Del(_) => "DEL",
            Command::Exists(_) => "EXISTS",
            Command::SetNx(_) => "SETNX",
            Command::MSet(_) => "MSET",
            Command::MGet(_) => "MGET",
            Command::Rename(_) => "RENAME",
            Command::Renamenx(_) => "RENAMENX",
            Command::Flushdb(_) => "FLUSHDB",
            Command::Strlen(_) => "STRLEN",
            Command::Append(_) => "APPEND",
            Command::GetRange(_) => "GETRANGE",
            Command::Incr(_) => "INCR",
            Command::Incrby(_) => "INCRBY",
            Command::Decr(_) => "DECR",
            Command::Decrby(_) => "DECRBY",
            Command::Incrbyfloat(_) => "INCRBYFLOAT",
            Command::Decrbyfloat(_) => "DECRBYFLOAT",
            Command::Setfloat(_) => "SETFLOAT",
            Command::HSet(_) => "HSET",
            Command::HGet(_) => "HGET",
            Command::HmGet(_) => "HMGET",
            Command::HDel(_) => "HDEL",
            Command::HExists(_) => "HEXISTS",
            Command::HLen(_) => "HLEN",
            Command::HKeys(_) => "HKEYS",
            Command::HVals(_) => "HVALS",
            Command::HGetall(_) => "HGETALL",
            Command::HRandField(_) => "HRANDFIELD",
            Command::HPfAdd(_) => "PFADD",
            Command::HPfCount(_) => "PFCOUNT",
            Command::HPfMerge(_) => "PFMERGE",
            Command::SAdd(_) => "SADD",
            Command::SRem(_) => "SREM",
            Command::SCard(_) => "SCARD",
            Command::SMembers(_) => "SMEMBERS",
            Command::SIsmember(_) => "SISMEMBER",
            Command::SRandMember(_) => "SRANDMEMBER",
            Command::SPop(_) => "SPOP",
            Command::SInter(_) => "SINTER",
            Command::SUnion(_) => "SUNION",
            Command::SDiff(_) => "SDIFF",
            Command::ZAdd(_) => "ZADD",
            Command::ZRem(_) => "ZREM",
            Command::ZRange(_) => "ZRANGE",
            Command::ZScore(_) => "ZSCORE",
            Command::ZCard(_) => "ZCARD",
            Command::ZRevrange(_) => "ZREVRANGE",
            Command::ZRank(_) => "ZRANK",
            Command::ZRevRank(_) => "ZREVRANK",
            Command::ZCount(_) => "ZCOUNT",
            Command::ZIncrBy(_) => "ZINCRBY",
            Command::LSet(_) => "LSET",
            Command::LRem(_) => "LREM",
            Command::LPush(_) => "LPUSH",
            Command::RPush(_) => "RPUSH",
            Command::LPop(_) => "LPOP",
            Command::RPop(_) => "RPOP",
            Command::LLen(_) => "LLEN",
            Command::LRange(_) => "LRANGE",
            Command::Auth(_) => "AUTH",
            Command::GeoAdd(_) => "GEOADD",
            Command::GeoDist(_) => "GEODIST",
            Command::GeoPos(_) => "GEOPOS",
            Command::GeoRadius(_) => "GEORADIUS",
            Command::GeoRadiusByMember(_) => "GEORADIUSBYMEMBER",
            Command::SetBit(_) => "SETBIT",
            Command::GetBit(_) => "GETBIT",
            Command::BitCount(_) => "BITCOUNT",
            Command::BitOp(_) => "BITOP",
            Command::BitPos(_) => "BITPOS",
            Command::Subscribe(_) => "SUBSCRIBE",
            Command::Unsubscribe(_) => "UNSUBSCRIBE",
            Command::Publish(_) => "PUBLISH",
            Command::PSubscribe(_) => "PSUBSCRIBE",
            Command::PUnsubscribe(_) => "PUNSUBSCRIBE",
            Command::PubSub(_) => "PUBSUB",
            Command::Ping(_) => "PING",
            Command::Echo(_) => "ECHO",
            Command::DbSize(_) => "DBSIZE",
            Command::Info(_) => "INFO",
            Command::Time(_) => "TIME",
            Command::Select(_) => "SELECT",
            Command::Save(_) => "SAVE",
            Command::BgSave(_) => "BGSAVE",
            Command::Shutdown(_) => "SHUTDOWN",
            Command::XAdd(_) => "XADD",
            Command::XRead(_) => "XREAD",
            Command::XRange(_) => "XRANGE",
            Command::XRevRange(_) => "XREVRANGE",
            Command::XLen(_) => "XLEN",
            Command::XDel(_) => "XDEL",
            Command::XTrim(_) => "XTRIM",
            Command::XGroupCreate(_) => "XGROUP CREATE",
            Command::XAck(_) => "XACK",
        }
    }

    /// Возвращает ключ команды (если есть).
    pub fn key(&self) -> Option<&[u8]> {
        match self {
            Command::Set(cmd) => Some(cmd.key.as_bytes()),
            Command::Get(cmd) => Some(cmd.key.as_bytes()),
            Command::SetNx(cmd) => Some(cmd.key.as_bytes()),
            Command::MSet(cmd) => cmd.entries.first().map(|(k, _)| k.as_bytes()), /* entries: Vec<(String, Value)> */
            Command::MGet(cmd) => cmd.keys.first().map(|k| k.as_bytes()),
            Command::Strlen(cmd) => Some(cmd.key.as_bytes()),
            Command::Append(cmd) => Some(cmd.key.as_bytes()),
            Command::GetRange(cmd) => Some(cmd.key.as_bytes()),
            Command::Del(cmd) => Some(cmd.key.as_bytes()),
            Command::Exists(cmd) => cmd.keys.first().map(|k| k.as_bytes()),
            Command::Rename(cmd) => Some(cmd.from.as_bytes()),
            Command::Renamenx(cmd) => Some(cmd.from.as_bytes()),
            Command::Flushdb(_) => None,
            Command::Incr(cmd) => Some(cmd.key.as_bytes()),
            Command::Incrby(cmd) => Some(cmd.key.as_bytes()),
            Command::Decr(cmd) => Some(cmd.key.as_bytes()),
            Command::Decrby(cmd) => Some(cmd.key.as_bytes()),
            Command::Incrbyfloat(cmd) => Some(cmd.key.as_bytes()),
            Command::Decrbyfloat(cmd) => Some(cmd.key.as_bytes()),
            Command::Setfloat(cmd) => Some(cmd.key.as_bytes()),
            Command::HSet(cmd) => Some(cmd.key.as_bytes()),
            Command::HGet(cmd) => Some(cmd.key.as_bytes()),
            Command::HmGet(cmd) => Some(cmd.key.as_bytes()),
            Command::HDel(cmd) => Some(cmd.key.as_bytes()),
            Command::HExists(cmd) => Some(cmd.key.as_bytes()),
            Command::HLen(cmd) => Some(cmd.key.as_bytes()),
            Command::HKeys(cmd) => Some(cmd.key.as_bytes()),
            Command::HVals(cmd) => Some(cmd.key.as_bytes()),
            Command::HGetall(cmd) => Some(cmd.key.as_bytes()),
            Command::HRandField(cmd) => Some(cmd.key.as_bytes()),
            Command::HPfAdd(cmd) => Some(cmd.key.as_bytes()),
            Command::HPfCount(cmd) => Some(cmd.key.as_bytes()),
            Command::HPfMerge(cmd) => Some(cmd.data.as_bytes()),
            Command::SAdd(cmd) => Some(cmd.key.as_bytes()),
            Command::SRem(cmd) => Some(cmd.key.as_bytes()),
            Command::SCard(cmd) => Some(cmd.key.as_bytes()),
            Command::SMembers(cmd) => Some(cmd.key.as_bytes()),
            Command::SIsmember(cmd) => Some(cmd.key.as_bytes()),
            Command::SRandMember(cmd) => Some(cmd.key.as_bytes()),
            Command::SPop(cmd) => Some(cmd.key.as_bytes()),
            Command::SInter(cmd) => cmd.keys.first().map(|k| k.as_bytes()),
            Command::SUnion(cmd) => cmd.keys.first().map(|k| k.as_bytes()),
            Command::SDiff(cmd) => cmd.keys.first().map(|k| k.as_bytes()),
            Command::ZAdd(cmd) => Some(cmd.key.as_bytes()),
            Command::ZRem(cmd) => Some(cmd.key.as_bytes()),
            Command::ZRange(cmd) => Some(cmd.key.as_bytes()),
            Command::ZScore(cmd) => Some(cmd.key.as_bytes()),
            Command::ZCard(cmd) => Some(cmd.key.as_bytes()),
            Command::ZRevrange(cmd) => Some(cmd.key.as_bytes()),
            Command::ZRank(cmd) => Some(cmd.key.as_bytes()),
            Command::ZRevRank(cmd) => Some(cmd.key.as_bytes()),
            Command::ZCount(cmd) => Some(cmd.key.as_bytes()),
            Command::ZIncrBy(cmd) => Some(cmd.key.as_bytes()),
            Command::LSet(cmd) => Some(cmd.key.as_bytes()),
            Command::LRem(cmd) => Some(cmd.key.as_bytes()),
            Command::LPush(cmd) => Some(cmd.key.as_bytes()),
            Command::RPush(cmd) => Some(cmd.key.as_bytes()),
            Command::LPop(cmd) => Some(cmd.key.as_bytes()),
            Command::RPop(cmd) => Some(cmd.key.as_bytes()),
            Command::LLen(cmd) => Some(cmd.key.as_bytes()),
            Command::LRange(cmd) => Some(cmd.key.as_bytes()),
            Command::Auth(_) => None,
            Command::GeoAdd(cmd) => Some(cmd.key.as_bytes()),
            Command::GeoDist(cmd) => Some(cmd.key.as_bytes()),
            Command::GeoPos(cmd) => Some(cmd.key.as_bytes()),
            Command::GeoRadius(cmd) => Some(cmd.key.as_bytes()),
            Command::GeoRadiusByMember(cmd) => Some(cmd.key.as_bytes()),
            Command::SetBit(cmd) => Some(cmd.key.as_bytes()),
            Command::GetBit(cmd) => Some(cmd.key.as_bytes()),
            Command::BitCount(cmd) => Some(cmd.key.as_bytes()),
            Command::BitOp(cmd) => cmd.keys.first().map(|k| k.as_bytes()),
            Command::BitPos(cmd) => Some(cmd.key.as_bytes()),
            Command::Subscribe(_) => None,
            Command::Unsubscribe(_) => None,
            Command::Publish(_) => None,
            Command::PSubscribe(_) => None,
            Command::PUnsubscribe(_) => None,
            Command::PubSub(_) => None,
            Command::Ping(_) => None,
            Command::Echo(_) => None,
            Command::DbSize(_) => None,
            Command::Info(_) => None,
            Command::Time(_) => None,
            Command::Select(_) => None,
            Command::Save(_) => None,
            Command::BgSave(_) => None,
            Command::Shutdown(_) => None,
            Command::XAdd(cmd) => Some(cmd.key.as_bytes()),
            Command::XRead(_) => None,
            Command::XRange(cmd) => Some(cmd.key.as_bytes()),
            Command::XRevRange(cmd) => Some(cmd.key.as_bytes()),
            Command::XLen(cmd) => Some(cmd.key.as_bytes()),
            Command::XDel(cmd) => Some(cmd.key.as_bytes()),
            Command::XTrim(cmd) => Some(cmd.key.as_bytes()),
            Command::XGroupCreate(cmd) => Some(cmd.key.as_bytes()),
            Command::XAck(cmd) => Some(cmd.key.as_bytes()),
        }
    }
}

impl CommandExecute for Command {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let start = Instant::now();
        let command_name = self.name();

        // Создаём tracker
        let mut tracker = SlowQueryTracker::new(command_name);

        // Добавляем key если есть
        if let Some(key) = self.key() {
            tracker.with_field("key", String::from_utf8_lossy(key));
        }

        // Выполняем команду
        let result = match self {
            Command::Set(cmd) => cmd.execute(store),
            Command::Get(cmd) => cmd.execute(store),
            Command::Del(cmd) => cmd.execute(store),
            Command::Exists(cmd) => cmd.execute(store),
            Command::SetNx(cmd) => cmd.execute(store),
            Command::MSet(cmd) => cmd.execute(store),
            Command::MGet(cmd) => cmd.execute(store),
            Command::Rename(cmd) => cmd.execute(store),
            Command::Renamenx(cmd) => cmd.execute(store),
            Command::Flushdb(cmd) => cmd.execute(store),
            Command::Strlen(cmd) => cmd.execute(store),
            Command::Append(cmd) => cmd.execute(store),
            Command::GetRange(cmd) => cmd.execute(store),
            Command::Incr(cmd) => cmd.execute(store),
            Command::Incrby(cmd) => cmd.execute(store),
            Command::Decr(cmd) => cmd.execute(store),
            Command::Decrby(cmd) => cmd.execute(store),
            Command::Incrbyfloat(cmd) => cmd.execute(store),
            Command::Decrbyfloat(cmd) => cmd.execute(store),
            Command::Setfloat(cmd) => cmd.execute(store),
            Command::HSet(cmd) => cmd.execute(store),
            Command::HGet(cmd) => cmd.execute(store),
            Command::HmGet(cmd) => cmd.execute(store),
            Command::HDel(cmd) => cmd.execute(store),
            Command::HExists(cmd) => cmd.execute(store),
            Command::HLen(cmd) => cmd.execute(store),
            Command::HKeys(cmd) => cmd.execute(store),
            Command::HVals(cmd) => cmd.execute(store),
            Command::HGetall(cmd) => cmd.execute(store),
            Command::HRandField(cmd) => cmd.execute(store),
            Command::HPfAdd(cmd) => cmd.execute(store),
            Command::HPfCount(cmd) => cmd.execute(store),
            Command::HPfMerge(cmd) => cmd.execute(store),
            Command::SAdd(cmd) => cmd.execute(store),
            Command::SRem(cmd) => cmd.execute(store),
            Command::SCard(cmd) => cmd.execute(store),
            Command::SMembers(cmd) => cmd.execute(store),
            Command::SIsmember(cmd) => cmd.execute(store),
            Command::SRandMember(cmd) => cmd.execute(store),
            Command::SPop(cmd) => cmd.execute(store),
            Command::SInter(cmd) => cmd.execute(store),
            Command::SUnion(cmd) => cmd.execute(store),
            Command::SDiff(cmd) => cmd.execute(store),
            Command::ZAdd(cmd) => cmd.execute(store),
            Command::ZRem(cmd) => cmd.execute(store),
            Command::ZRange(cmd) => cmd.execute(store),
            Command::ZScore(cmd) => cmd.execute(store),
            Command::ZCard(cmd) => cmd.execute(store),
            Command::ZRevrange(cmd) => cmd.execute(store),
            Command::ZRank(cmd) => cmd.execute(store),
            Command::ZRevRank(cmd) => cmd.execute(store),
            Command::ZCount(cmd) => cmd.execute(store),
            Command::ZIncrBy(cmd) => cmd.execute(store),
            Command::LSet(cmd) => cmd.execute(store),
            Command::LRem(cmd) => cmd.execute(store),
            Command::LPush(cmd) => cmd.execute(store),
            Command::RPush(cmd) => cmd.execute(store),
            Command::LPop(cmd) => cmd.execute(store),
            Command::RPop(cmd) => cmd.execute(store),
            Command::LLen(cmd) => cmd.execute(store),
            Command::LRange(cmd) => cmd.execute(store),
            Command::Auth(cmd) => cmd.execute(store),
            Command::GeoAdd(cmd) => cmd.execute(store),
            Command::GeoDist(cmd) => cmd.execute(store),
            Command::GeoPos(cmd) => cmd.execute(store),
            Command::GeoRadius(cmd) => cmd.execute(store),
            Command::GeoRadiusByMember(cmd) => cmd.execute(store),
            Command::SetBit(cmd) => cmd.execute(store),
            Command::GetBit(cmd) => cmd.execute(store),
            Command::BitCount(cmd) => cmd.execute(store),
            Command::BitOp(cmd) => cmd.execute(store),
            Command::BitPos(cmd) => cmd.execute(store),
            Command::Subscribe(cmd) => cmd.execute(store),
            Command::Unsubscribe(cmd) => cmd.execute(store),
            Command::Publish(cmd) => cmd.execute(store),
            Command::PSubscribe(cmd) => cmd.execute(store),
            Command::PUnsubscribe(cmd) => cmd.execute(store),
            Command::PubSub(cmd) => cmd.execute(store),
            Command::Ping(cmd) => cmd.execute(store),
            Command::Echo(cmd) => cmd.execute(store),
            Command::DbSize(cmd) => cmd.execute(store),
            Command::Info(cmd) => cmd.execute(store),
            Command::Time(cmd) => cmd.execute(store),
            Command::Select(cmd) => cmd.execute(store),
            Command::Save(cmd) => cmd.execute(store),
            Command::BgSave(cmd) => cmd.execute(store),
            Command::Shutdown(cmd) => cmd.execute(store),
            Command::XAdd(cmd) => cmd.execute(store),
            Command::XRead(cmd) => cmd.execute(store),
            Command::XRange(cmd) => cmd.execute(store),
            Command::XRevRange(cmd) => cmd.execute(store),
            Command::XLen(cmd) => cmd.execute(store),
            Command::XDel(cmd) => cmd.execute(store),
            Command::XTrim(cmd) => cmd.execute(store),
            Command::XGroupCreate(cmd) => cmd.execute(store),
            Command::XAck(cmd) => cmd.execute(store),
        };

        // Добавляем result / error в tracker (используем ссылку, чтобы не перемещать
        // `result`)
        match &result {
            Ok(_) => {
                tracker.with_field("result", "success");
            }
            Err(err) => {
                tracker.with_field("result", "error");
                tracker.with_field("error", format!("{err:?}"));
            }
        }

        // Измеряем длительность и кладём в трекер (мс)
        let elapsed_ms = start.elapsed().as_millis();
        tracker.with_field("duration_ms", format!("{elapsed_ms}"));

        // tracker.finish() вызывается автоматически при drop
        // если команда медленная - она будет залогирована

        result
    }

    fn command_name(&self) -> &'static str {
        self.name()
    }

    fn command_key(&self) -> Option<&[u8]> {
        self.key()
    }
}

impl CommandExecutor {
    pub fn new() -> Self {
        Self {
            client_addr: None,
            slot_id: None,
        }
    }

    pub fn with_client_addr(
        mut self,
        addr: String,
    ) -> Self {
        self.client_addr = Some(addr);
        self
    }

    pub fn with_slot_id(
        mut self,
        slot_id: u16,
    ) -> Self {
        self.slot_id = Some(slot_id);
        self
    }

    /// Выполняет команду с slow query tracking и контекстом.
    pub fn execute(
        &self,
        command: &Command,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let start = Instant::now();
        let command_name = command.name();

        // Создаём tracker с полным контекстом
        let mut tracker = SlowQueryTracker::new(command_name);

        if let Some(ref addr) = self.client_addr {
            tracker.with_field("client_addr", addr);
        }

        if let Some(slot) = self.slot_id {
            tracker.with_field("slot_id", slot);
        }

        if let Some(key) = command.key() {
            tracker.with_field("key", String::from_utf8_lossy(key));
        }

        // Выполняем команду
        let result = command.execute(store);

        // Добавляем результат
        match result {
            Ok(_) => {
                tracker.with_field("result", "success");
            }
            Err(ref e) => {
                tracker.with_field("result", "error");
                tracker.with_field("error", format!("{e:?}"));
            }
        }

        // Измеряем длительность и кладём в трекер (мс)
        let elapsed_ms = start.elapsed().as_millis();
        tracker.with_field("duration_ms", format!("{elapsed_ms}"));

        result
    }
}

impl Default for CommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}
