use crate::{database::types::Value, engine::engine::StorageEngine, error::StoreError};

use super::{GetCommand, SetCommand};

pub trait CommandExecute {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError>;
}

#[derive(Debug)]
pub enum Command {
    Set(SetCommand),
    Get(GetCommand),
}

impl CommandExecute for Command {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        match self {
            Command::Set(cmd) => cmd.execute(store),
            Command::Get(cmd) => cmd.execute(store),
        }
    }
}
