#[derive(Debug, Clone)]
pub enum Command {
    Ping,
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
