use crate::database::Value;

#[derive(Debug, Clone)]
pub enum Command {
    // --- Простые команды ---//
    Ping,
    Echo(String),

    // --- String ---
    Set { key: String, value: Value },
    Get { key: String },
    Del { key: String },
}

#[derive(Debug, Clone)]
pub enum Response {
    Ok,
    Value(Value),
    Error(String),
}
