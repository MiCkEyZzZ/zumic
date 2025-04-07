use std::collections::HashMap;

/// Представляет базовые типы протокола ZSP (Zumic Serialization Protocol).
#[derive(Debug, Clone, PartialEq)]
pub enum ZSPFrame {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),                   // None для Null
    Array(Option<Vec<ZSPFrame>>),                  // None для Null
    Dictionary(Option<HashMap<String, ZSPFrame>>), // Новый тип для словарей
}
