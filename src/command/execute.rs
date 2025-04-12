use crate::{database::types::Value, engine::engine::StorageEngine, error::StoreError};

use super::{
    AppendCommand, DecrByCommand, DecrCommand, DelCommand, ExistsCommand, FlushDbCommand,
    GetCommand, GetRangeCommand, IncrByCommand, IncrCommand, MGetCommand, MSetCommand,
    RenameCommand, RenameNxCommand, SetCommand, SetNxCommand, StrLenCommand,
};

pub trait CommandExecute: std::fmt::Debug {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError>;
}

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
}

impl CommandExecute for Command {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
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
        }
    }
}
