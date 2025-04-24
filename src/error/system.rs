use std::io;

use thiserror::Error;

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Error, Debug)]
pub enum StoreError {
    // ==== Система / Внешний ====
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("UTF-8 decoding failed: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Lua VM error: {0}")]
    Lua(#[from] mlua::Error),

    // ==== Ошибки команды ====
    #[error("Invalid command: {0}")]
    InvalidCommand(String),

    #[error("Wrong type for operation: {0}")]
    WrongType(String),

    #[error("Key not found")]
    KeyNotFound,

    #[error("Index out of bounds")]
    IndexOutOfBounds,

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Invalid type")]
    InvalidType,

    #[error("Operation not implemented: {0}")]
    NotImplemented(String),

    // ==== Сеть и кластер ====
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Cluster state error: {0}")]
    Cluster(String),

    // ==== PubSub ====
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("Subscriber error: {0}")]
    Subscriber(String),

    // ==== Общие сведения ====
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Lock acquisition failed: {0}")]
    LockError(String),

    #[error("Invalid key encoding")]
    InvalidKey,

    #[error("File processing error")]
    FileError,
}
