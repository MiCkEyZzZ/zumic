use crate::database::types::Value;

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
    SetNX { key: String, value: Value },
    Rename { from: String, to: String },
    RenameNX { from: String, to: String },

    // Авторизация
    Auth { user: Option<String>, pass: String },
}

#[derive(Debug, Clone)]
pub enum Response {
    Ok,
    Value(Value),
    Error(String),
    NotFound,
    Integer(i64),
    Float(f64),
    String(String),
}
