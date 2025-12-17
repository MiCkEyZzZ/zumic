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

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет: ConnectionFailed — код статуса, клиентское сообщение и
    /// метки.
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

    /// Тест проверяет: ConnectionClosed с причиной и без неё.
    #[test]
    fn test_connection_closed() {
        let err_with_reason = NetworkError::ConnectionClosed {
            reason: Some("remote reset".into()),
        };
        assert_eq!(err_with_reason.status_code(), StatusCode::ConnectionClosed);
        assert!(err_with_reason.to_string().contains("remote reset"));

        let err_without_reason = NetworkError::ConnectionClosed { reason: None };
        assert_eq!(err_without_reason.client_message(), "Connection closed");
        assert!(err_without_reason.to_string().contains("Connection closed"));
    }

    /// Тест проверяет: ReadTimeout и WriteTimeout — статус и сообщение.
    #[test]
    fn test_read_write_timeout() {
        let read = NetworkError::ReadTimeout;
        assert_eq!(read.status_code(), StatusCode::ReadTimeout);
        assert_eq!(read.client_message(), "Read timeout");
        assert!(read.to_string().contains("Read timeout"));

        let write = NetworkError::WriteTimeout;
        assert_eq!(write.status_code(), StatusCode::WriteTimeout);
        assert_eq!(write.client_message(), "Write timeout");
        assert!(write.to_string().contains("Write timeout"));
    }

    /// Тест проверяет: UnexpectedResponse — статус, клиентское сообщение и
    /// вывод Display.
    #[test]
    fn test_unexpected_response() {
        let err = NetworkError::UnexpectedResponse {
            expected: "PONG".into(),
            got: "ERR".into(),
        };
        assert_eq!(err.status_code(), StatusCode::ProtocolError);
        assert_eq!(err.client_message(), "Unexpected server response");
        assert!(err.to_string().contains("expected PONG"));
    }

    /// Тест проверяет: ProtocolError — статус, сообщение и вывод Display.
    #[test]
    fn test_protocol_error() {
        let err = NetworkError::ProtocolError {
            reason: "invalid frame".into(),
        };
        assert_eq!(err.status_code(), StatusCode::ProtocolError);
        assert_eq!(err.client_message(), "Protocol error");
        assert!(err.to_string().contains("Protocol error"));
    }

    /// Тест проверяет: TooManyConnections — статус, retryable флаг, метки
    /// current/max и сообщение.
    #[test]
    fn test_too_many_connections() {
        let err = NetworkError::TooManyConnections {
            current: 100,
            max: 100,
        };
        assert_eq!(err.status_code(), StatusCode::TooManyConnections);
        assert_eq!(err.client_message(), "Too many connections");
        assert!(err.to_string().contains("100/100"));
        let tags = err.metrics_tags();
        assert!(tags.iter().any(|(k, _)| *k == "current_connections"));
        assert!(tags.iter().any(|(k, _)| *k == "max_connections"));
        assert!(err.status_code().is_retryable());
    }

    /// Тест проверяет: ParseError, EncodingError и DecodingError — коды
    /// статусов, сообщения и форматирование.
    #[test]
    fn test_parse_and_codec_errors() {
        let parse = NetworkError::ParseError {
            reason: "bad syntax".into(),
        };
        assert_eq!(parse.status_code(), StatusCode::ParseError);
        assert_eq!(parse.client_message(), "Invalid command format");
        assert!(parse.to_string().contains("Parse error"));

        let enc = NetworkError::EncodingError {
            reason: "utf8 invalid".into(),
        };
        assert_eq!(enc.status_code(), StatusCode::EncodingError);
        assert_eq!(enc.client_message(), "Encoding error");
        assert!(enc.to_string().contains("Encoding error"));

        let dec = NetworkError::DecodingError {
            reason: "truncated".into(),
        };
        assert_eq!(dec.status_code(), StatusCode::DecodingError);
        assert_eq!(dec.client_message(), "Decoding error");
        assert!(dec.to_string().contains("Decoding error"));
    }

    /// Тест проверяет: IncompleteData — статус, сообщение и корректность
    /// вывода.
    #[test]
    fn test_incomplete_data() {
        let err = NetworkError::IncompleteData {
            expected: 1024,
            got: 512,
        };
        assert_eq!(err.status_code(), StatusCode::UnexpectedEof);
        assert_eq!(err.client_message(), "Incomplete data received");
        assert!(err.to_string().contains("expected 1024"));
    }

    /// Тест проверяет: as_any даёт возможность безопасного downcast к
    /// NetworkError.
    #[test]
    fn test_as_any_downcast() {
        let err = NetworkError::WriteTimeout;
        let any_ref = err.as_any();
        assert!(any_ref.downcast_ref::<NetworkError>().is_some());
    }

    /// Тест проверяет: базовые теги metrics_tags для всех вариантов содержат
    /// error_type и status_code.
    #[test]
    fn test_common_metrics_tags() {
        let err = NetworkError::ProtocolError {
            reason: "bad header".into(),
        };
        let tags = err.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| k == &"error_type" && v == "network"));
        assert!(tags.iter().any(|(k, _)| k == &"status_code"));
    }
}
