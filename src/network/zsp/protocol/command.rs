#[derive(Debug, Clone)]
pub enum Command {
    // --- Простые команды ---//
    Ping,
    Echo(String),

    // --- String ---
    Set { key: String, value: Option<Vec<u8>> },
    Get { key: String },
    Del { key: String },
}

#[derive(Debug, Clone)]
pub enum Response {
    Ok,
    Value(Option<Vec<u8>>),
    Error(String),
}
