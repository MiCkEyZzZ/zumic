use crate::Value;

#[derive(Debug, Clone)]
pub enum Command {
    // --- Простые команды ---//
    Ping,
    Echo(String),

    // --- Базовые ---
    Set {
        key: String,
        value: Value,
    },
    Get {
        key: String,
    },
    Del {
        key: String,
    },
    MSet {
        entries: Vec<(String, Value)>,
    },
    MGet {
        keys: Vec<String>,
    },
    SetNx {
        key: String,
        value: Value,
    },
    Rename {
        from: String,
        to: String,
    },
    RenameNx {
        from: String,
        to: String,
    },

    // Авторизация
    Auth {
        user: Option<String>,
        pass: String,
    },

    // --- PubSub команды ---
    Subscribe {
        channels: Vec<String>,
    },
    Unsubscribe {
        channels: Vec<String>,
    },
    Publish {
        channel: String,
        message: PubSubMessage,
    },
}

// Новый тип для pub/sub сообщений
#[derive(Debug, Clone, PartialEq)]
pub enum PubSubMessage {
    Bytes(Vec<u8>),
    String(String),
    Json(serde_json::Value),
    Serialized { data: Vec<u8>, content_type: String },
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

            // PubSub команды
            Command::Subscribe { .. } => "subscribe",
            Command::Unsubscribe { .. } => "unsubscribe",
            Command::Publish { .. } => "publish",
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

    // PubSub ответы
    Message {
        channel: String,
        message: PubSubMessage,
    },
    Subscribed {
        channel: String,
        count: i64,
    },
    Unsubscribed {
        channel: String,
        count: i64,
    },
}
