//! Абстракция и диспетчеризация команд Zumic.
//!
//! Содержит трейт [`CommandExecute`] для унифицированного выполнения команд
//! и enum [`Command`], инкапсулирующий все поддерживаемые команды.
//! Это позволяет обрабатывать любые команды через единый интерфейс.

use super::{
    AppendCommand, AuthCommand, BitCountCommand, BitOpCommand, DecrByCommand, DecrCommand,
    DelCommand, ExistsCommand, FlushDbCommand, GeoAddCommand, GeoPosCommand,
    GeoRadiusByMemberCommand, GeoRadiusCommand, GetBitCommand, GetCommand, GetDistCommand,
    GetRangeCommand, HDelCommand, HGetAllCommand, HGetCommand, HSetCommand, IncrByCommand,
    IncrByFloatCommand, IncrCommand, LLenCommand, LPopCommand, LPushCommand, LRangeCommand,
    MGetCommand, MSetCommand, RPopCommand, RPushCommand, RenameCommand, RenameNxCommand,
    SAddCommand, SCardCommand, SIsMemberCommand, SMembersCommand, SRemCommand, SetBitCommand,
    SetCommand, SetFloatCommand, SetNxCommand, StrLenCommand, ZAddCommand, ZCardCommand,
    ZRangeCommand, ZRemCommand, ZRevRangeCommand, ZScoreCommand,
};
use crate::{
    command::pubsub::{PublishCommand, SubscribeCommand, UnsubscribeCommand},
    StorageEngine, StoreError, Value,
};

pub trait CommandExecute: std::fmt::Debug {
    /// Выполняет команду, взаимодействуя с хранилищем.
    ///
    /// Метод изменяет состояние хранилища (если команда подразумевает изменения)
    /// и возвращает результат выполнения.
    ///
    /// # Параметры
    /// - `store` — ссылка на хранилище, над которым выполняется команда.
    ///
    /// # Возвращает
    /// - `Ok(Value)` — результат выполнения команды (например, полученное значение, количество, статус и т.д.).
    /// - `Err(StoreError)` — если произошла ошибка при выполнении (например, неверный тип, отсутствие ключа, сбой хранилища).
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError>;
}

/// Перечисление всех поддерживаемых команд Zumic.
///
/// Каждый вариант содержит структуру соответствующей команды.
/// Enum реализует [`CommandExecute`], что позволяет выполнять любую команду
/// через единый интерфейс.
///
/// Обычно используется для парсинга и диспетчеризации команд, полученных от клиента.
#[derive(Debug)]
pub enum Command {
    Set(SetCommand),
    Get(GetCommand),
    Del(DelCommand),
    Exists(ExistsCommand),
    Setnx(SetNxCommand),
    MSet(MSetCommand),
    MGet(MGetCommand),
    Rename(RenameCommand),
    Renamenx(RenameNxCommand),
    Flushdb(FlushDbCommand),
    Strlen(StrLenCommand),
    Append(AppendCommand),
    Getrange(GetRangeCommand),
    Incr(IncrCommand),
    Incrby(IncrByCommand),
    Decr(DecrCommand),
    Decrby(DecrByCommand),
    Incrbyfloat(IncrByFloatCommand),
    Decrbyfloat(DecrByCommand),
    Setfloat(SetFloatCommand),
    HSet(HSetCommand),
    HGet(HGetCommand),
    HDel(HDelCommand),
    HGetall(HGetAllCommand),
    SAdd(SAddCommand),
    SRem(SRemCommand),
    SIsmember(SIsMemberCommand),
    SMembers(SMembersCommand),
    SCard(SCardCommand),
    ZAdd(ZAddCommand),
    ZScore(ZScoreCommand),
    ZCard(ZCardCommand),
    ZRem(ZRemCommand),
    ZRange(ZRangeCommand),
    ZRevrange(ZRevRangeCommand),
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
    Subscribe(SubscribeCommand),
    Unsubscribe(UnsubscribeCommand),
    Publish(PublishCommand),
}

impl CommandExecute for Command {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        match self {
            Command::Set(cmd) => cmd.execute(store),
            Command::Get(cmd) => cmd.execute(store),
            Command::Del(cmd) => cmd.execute(store),
            Command::Exists(cmd) => cmd.execute(store),
            Command::Setnx(cmd) => cmd.execute(store),
            Command::MSet(cmd) => cmd.execute(store),
            Command::MGet(cmd) => cmd.execute(store),
            Command::Rename(cmd) => cmd.execute(store),
            Command::Renamenx(cmd) => cmd.execute(store),
            Command::Flushdb(cmd) => cmd.execute(store),
            Command::Strlen(cmd) => cmd.execute(store),
            Command::Append(cmd) => cmd.execute(store),
            Command::Getrange(cmd) => cmd.execute(store),
            Command::Incr(cmd) => cmd.execute(store),
            Command::Incrby(cmd) => cmd.execute(store),
            Command::Decr(cmd) => cmd.execute(store),
            Command::Decrby(cmd) => cmd.execute(store),
            Command::Incrbyfloat(cmd) => cmd.execute(store),
            Command::Decrbyfloat(cmd) => cmd.execute(store),
            Command::Setfloat(cmd) => cmd.execute(store),
            Command::HSet(cmd) => cmd.execute(store),
            Command::HGet(cmd) => cmd.execute(store),
            Command::HDel(cmd) => cmd.execute(store),
            Command::HGetall(cmd) => cmd.execute(store),
            Command::SAdd(cmd) => cmd.execute(store),
            Command::SRem(cmd) => cmd.execute(store),
            Command::SIsmember(cmd) => cmd.execute(store),
            Command::SMembers(cmd) => cmd.execute(store),
            Command::SCard(cmd) => cmd.execute(store),
            Command::ZAdd(cmd) => cmd.execute(store),
            Command::ZScore(cmd) => cmd.execute(store),
            Command::ZCard(cmd) => cmd.execute(store),
            Command::ZRem(cmd) => cmd.execute(store),
            Command::ZRange(cmd) => cmd.execute(store),
            Command::ZRevrange(cmd) => cmd.execute(store),
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
            Command::Subscribe(cmd) => cmd.execute(store),
            Command::Unsubscribe(cmd) => cmd.execute(store),
            Command::Publish(cmd) => cmd.execute(store),
        }
    }
}
