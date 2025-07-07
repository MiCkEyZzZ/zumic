use crate::Value;

#[derive(Debug, Clone)]
pub enum Command {
    // --- Простые команды ---//
    Ping,
    Echo(String),

    // --- Базовые ---
    Set { key: String, value: Value },
    Get { key: String },
    Del { key: String },
    MSet { entries: Vec<(String, Value)> },
    MGet { keys: Vec<String> },
    SetNx { key: String, value: Value },
    Rename { from: String, to: String },
    RenameNx { from: String, to: String },

    // Авторизация
    Auth { user: Option<String>, pass: String },
}

impl Command {
    /// Возвращает имя команды (тот самый ключ для Registry).
    pub fn name(&self) -> &'static str {
        match self {
            Command::Ping => "ping",
            Command::Echo(_) => "echo",
            Command::Set { .. } => "set",
            Command::Get { .. } => "get",
            Command::Del { .. } => "del",
            Command::MSet { .. } => "mset",
            Command::MGet { .. } => "mget",
            Command::SetNx { .. } => "setnx",
            Command::Rename { .. } => "rename",
            Command::RenameNx { .. } => "renamenx",
            Command::Auth { .. } => "auth",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Response {
    Ok,
    Value(Value),
    Error(String),
    NotFound,
    Integer(i64),
    Float(f64),
    Bool(bool),
    String(String),
}

impl Response {
    pub fn error(msg: impl Into<String>) -> Self {
        Response::Error(msg.into())
    }
}
