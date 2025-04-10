use crate::{database::types::Value, engine::engine::StorageEngine, error::StoreError};

use super::{
    DelCommand, ExistsCommand, FlushDbCommand, GetCommand, MGetCommand, MSetCommand, RenameCommand,
    RenameNxCommand, SetCommand, SetNxCommand,
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
        }
    }
}
