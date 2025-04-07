use std::collections::HashMap;

/// Представляет один фрейм (единицу данных) протокола ZSP (Zumic Serialization Protocol).
///
/// Используется для сериализации/десериализации данных между клиентом и сервером.
/// Протокол вдохновлён RESP3 (Redis), но расширен — в частности, поддержкой словарей.
///
/// ## Типы фреймов:
///
/// - `SimpleString(String)` — простая строка, не содержащая символов `\r` или `\n`, например, `+OK\r\n`.
/// - `Error(String)` — строка ошибки, которая может быть использована для передачи ошибок клиенту, например, `-ERR something went wrong\r\n`.
/// - `Integer(i64)` — целое число, представляемое как `:42\r\n`.
/// - `BulkString(Option<Vec<u8>>)` — бинарные данные, которые могут быть `null`. Представляется как `$<len>\r\n<bytes>\r\n` или `$-1\r\n` для `None`.
/// - `Array(Option<Vec<ZSPFrame>>)` — массив фреймов, который может быть `null`. Представляется как `*<len>\r\n<frame1>...<frameN>` или `*-1\r\n` для `None`.
/// - `Dictionary(Option<HashMap<String, ZSPFrame>>)` — словарь, где ключи — это строки, а значения могут быть любыми фреймами ZSP. Представляется как `%<len>\r\n+key\r\n<value>\r\n...` или `%-1\r\n` для `None`.
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
