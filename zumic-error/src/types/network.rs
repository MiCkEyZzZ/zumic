use std::any::Any;

use crate::{ErrorExt, StatusCode};

/// Ошибки сетевого подключения и передачи данных.
#[derive(Debug, Clone)]
pub enum NetworkError {
    /// Не удалось подключиться
    ConnectionFailed { address: String, reason: String },
    /// Таймаут подключения
    ConnectionTimeout { address: String },
    /// Соединение закрыто
    ConnectionClosed { reason: Option<String> },
    /// Таймаут чтения
    ReadTimeout,
    /// Таймаут записи
    WriteTimeout,
    /// Неожиданный ответ от сервера
    UnexpectedResponse { expected: String, got: String },
    /// Ошибка протокола
    ProtocolError { reason: String },
    /// Слишком много соединений
    TooManyConnections { current: usize, max: usize },
    /// Ошибка парсинга
    ParseError { reason: String },
    /// Ошибка кодирования
    EncodingError { reason: String },
    /// Огибка декодирования
    DecodingError { reason: String },
    /// Неполные данные
    IncompleteData { expected: usize, got: usize },
}

impl std::fmt::Display for NetworkError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed { address, reason } => {
                write!(f, "Failed to connect to {address}: {reason}")
            }
            Self::ConnectionTimeout { address } => write!(f, "Connection timeout to {address}"),
            Self::ConnectionClosed { reason } => {
                if let Some(r) = reason {
                    write!(f, "Connection closed: {r}")
                } else {
                    write!(f, "Connection closed")
                }
            }
            Self::ReadTimeout => write!(f, "Read timeout"),
            Self::WriteTimeout => write!(f, "Write timeout"),
            Self::UnexpectedResponse { expected, got } => {
                write!(f, "Unexpected response: expected {expected}, got {got}")
            }
            Self::ProtocolError { reason } => write!(f, "Protocol error: {reason}"),
            Self::TooManyConnections { current, max } => {
                write!(f, "Too many connections: {current}/{max}")
            }
            Self::ParseError { reason } => write!(f, "Parse error: {reason}"),
            Self::EncodingError { reason } => write!(f, "Encoding error: {reason}"),
            Self::DecodingError { reason } => write!(f, "Decoding error: {reason}"),
            Self::IncompleteData { expected, got } => {
                write!(f, "Incomplete data: expected {expected} bytes, got {got}")
            }
        }
    }
}

impl std::error::Error for NetworkError {}

impl ErrorExt for NetworkError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::ConnectionFailed { .. } => StatusCode::ConnectionFailed,
            Self::ConnectionTimeout { .. } => StatusCode::Timeout,
            Self::ConnectionClosed { .. } => StatusCode::ConnectionClosed,
            Self::ReadTimeout => StatusCode::ReadTimeout,
            Self::WriteTimeout => StatusCode::WriteTimeout,
            Self::UnexpectedResponse { .. } | Self::ProtocolError { .. } => {
                StatusCode::ProtocolError
            }
            Self::TooManyConnections { .. } => StatusCode::TooManyConnections,
            Self::ParseError { .. } => StatusCode::ParseError,
            Self::EncodingError { .. } => StatusCode::EncodingError,
            Self::DecodingError { .. } => StatusCode::DecodingError,
            Self::IncompleteData { .. } => StatusCode::UnexpectedEof,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::ConnectionFailed { .. } => "Connection failed".to_string(),
            Self::ConnectionTimeout { .. } => "Connection timeout".to_string(),
            Self::ConnectionClosed { .. } => "Connection closed".to_string(),
            Self::ReadTimeout => "Read timeout".to_string(),
            Self::WriteTimeout => "Write timeout".to_string(),
            Self::UnexpectedResponse { .. } => "Unexpected server response".to_string(),
            Self::ProtocolError { .. } => "Protocol error".to_string(),
            Self::TooManyConnections { .. } => "Too many connections".to_string(),
            Self::ParseError { .. } => "Invalid command format".to_string(),
            Self::EncodingError { .. } => "Encoding error".to_string(),
            Self::DecodingError { .. } => "Decoding error".to_string(),
            Self::IncompleteData { .. } => "Incomplete data received".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "network".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::ConnectionFailed { address, .. } | Self::ConnectionTimeout { address } => {
                tags.push(("address", address.clone()));
            }
            Self::TooManyConnections { current, max } => {
                tags.push(("current_connections", current.to_string()));
                tags.push(("max_connections", max.to_string()));
            }
            _ => {}
        }

        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_failed() {
        let err = NetworkError::ConnectionFailed {
            address: "127.0.0.1:6379".to_string(),
            reason: "Connection refused".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::ConnectionFailed);

        let tags = err.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| k == &"address" && v == "127.0.0.1:6379"));
    }

    #[test]
    fn test_too_many_connections() {
        let err = NetworkError::TooManyConnections {
            current: 1000,
            max: 1000,
        };
        assert_eq!(err.status_code(), StatusCode::TooManyConnections);
        assert!(err.status_code().is_retryable());
    }
}
