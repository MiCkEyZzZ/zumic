//! Ошибки клиента Zumic
//!
//! Определяет типы ошибок, специфичные для клиентской части.

use std::io;

use thiserror::Error;

pub type ClientResult<T> = Result<T, ClientError>;

/// Ошибки клиента Zumic.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Ошибка подключения к серверу
    #[error("Не удалось подключиться к {0}: {1}")]
    ConnectionFailed(String, String),
    /// Таймаут подключения
    #[error("Таймаут подключения к серверу")]
    ConnectionTimeout,
    /// Соединение закрыто сервером
    #[error("Соединение закрыто сервером")]
    ConnectionClosed,
    /// Ошибка от сервера
    #[error("Ошибка сервера: {0}")]
    ServerError(String),
    /// Неверная команда
    #[error("Неверная команда")]
    InvalidCommand,
    /// Неизвестная команда
    #[error("Неизвестная команда: {0}")]
    UnknownCommand(String),
    /// Неожиданный ответ от сервера
    #[error("Неожиданный ответ от сервера")]
    UnexpectedResponse,
    /// Ошибка аутентификации
    #[error("Ошибка аутентификации: {0}")]
    AuthenticationFailed(String),
    /// Ошибка ввода-вывода
    #[error("Ошибка ввода-вывода: {0}")]
    Io(#[from] io::Error),
    /// Ошибка протокола ZSP
    #[error("Ошибка протокола: {0}")]
    Protocol(String),
    /// Ошибка кодирования ZSP
    #[error("Ошибка кодирования: {0}")]
    EncodingError(String),
    /// Ошибка декодирования ZSP
    #[error("Ошибка декодирования: {0}")]
    DecodingError(String),
    /// Неполные данные (ожидание продолжения)
    #[error("Неполные данные, ожидание продолжения")]
    IncompleteData,
    /// Таймаут чтения
    #[error("Таймаут чтения данных")]
    ReadTimeout,
    /// Таймаут записи
    #[error("Таймаут записи данных")]
    WriteTimeout,
}
