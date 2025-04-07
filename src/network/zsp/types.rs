/// Представляет базовые типы протокола ZSP (Zumic Serialization Protocol).
#[derive(Debug, Clone, PartialEq)]
pub enum ZSPFrame {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<ZSPFrame>>),
}
