use std::collections::HashMap;

/// Представляет один фрейм (единицу данных) протокола ZSP (Zumic Serialization Protocol).
///
/// Используется для сериализации/десериализации данных между клиентом и сервером.
#[derive(Debug, Clone, PartialEq)]
pub enum ZSPFrame {
    /// Простая строка. Не должна содержать `\r` или `\n`.
    SimpleString(String),
    /// Ошибка. Используется для передачи ошибок клиенту.
    Error(String),
    /// Целое число (i64).
    Integer(i64),
    /// Бинарная строка (может быть null).
    ///
    /// None → `$-1\r\n`
    /// Some(vec) → `$<len>\r\n<bytes>\r\n`
    BulkString(Option<Vec<u8>>),
    /// Массив вложенных фреймов (может быть null).
    ///
    /// None → `*-1\r\n`
    /// Some(vec) → `*<len>\r\n<frame1>...<frameN>`
    Array(Option<Vec<ZSPFrame>>),
    /// Словарь `ключ:значение`, где ключ — всегда строка, а значение — любой `ZSPFrame`.
    ///
    /// None → `%-1\r\n`
    /// Some(map) → `%<len>\r\n+key\r\n<value>\r\n...`
    Dictionary(Option<HashMap<String, ZSPFrame>>),
}
