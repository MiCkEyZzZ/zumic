use std::io;

use crate::{ErrorExt, StatusCode};

/// Ошибки клиента.
#[derive(Debug, Clone)]
pub enum ClientError {
    /// Ошибка подключения к серверу
    ConnectionFailed { address: String, reason: String },
    /// Таймаут подключения
    ConnectionTimeout,
    /// Соединение закрыто сервером
    ConnectionClosed,
    /// Ошибка от сервера
    ServerError { message: String },
    /// Неверная команда
    InvalidCommand,
    /// Неизвестная команда
    UnknownCommand { command: String },
    /// Неожиданный ответ от сервера
    UnexpectedResponse,
    /// Ошибка аутентификации
    AuthenticationFailed { reason: String },
    /// Ошибка ввода-вывода
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    /// Ошибка протокола ZSP
    Protocol { reason: String },
    /// Ошибка кодирования ZSP
    EncodingError { reason: String },
    /// Ошибка декодирования ZSP
    DecodingError { reason: String },
    /// Неполные данные (ожидание продолжения)
    IncompleteData,
    /// Таймаут чтения
    ReadTimeout,
    /// Таймаут записи
    WriteTimeout,
}

impl std::fmt::Display for ClientError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed { address, reason } => {
                write!(f, "Failed to connect to {address}: {reason}")
            }
            Self::ConnectionTimeout => write!(f, "Connection timeout"),
            Self::ConnectionClosed => write!(f, "Connection closed by server"),
            Self::ServerError { message } => write!(f, "Server error: {message}"),
            Self::InvalidCommand => write!(f, "Invalid command"),
            Self::UnknownCommand { command } => write!(f, "Unknown command: {command}"),
            Self::UnexpectedResponse => write!(f, "Unexpected response from server"),
            Self::AuthenticationFailed { reason } => {
                write!(f, "Authentication failed: {reason}")
            }
            Self::Io { kind, message } => write!(f, "I/O error ({kind:?}): {message}"),
            Self::Protocol { reason } => write!(f, "Protocol error: {reason}"),
            Self::EncodingError { reason } => write!(f, "Encoding error: {reason}"),
            Self::DecodingError { reason } => write!(f, "Decoding error: {reason}"),
            Self::IncompleteData => write!(f, "Incomplete data, waiting for continuation"),
            Self::ReadTimeout => write!(f, "Read timeout"),
            Self::WriteTimeout => write!(f, "Write timeout"),
        }
    }
}

impl std::error::Error for ClientError {}

impl ErrorExt for ClientError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::ConnectionFailed { .. } | Self::ConnectionTimeout | Self::ConnectionClosed => {
                StatusCode::ConnectionFailed
            }
            Self::ServerError { .. } => StatusCode::Internal,
            Self::InvalidCommand | Self::UnknownCommand { .. } => StatusCode::InvalidCommand,
            Self::UnexpectedResponse => StatusCode::ProtocolError,
            Self::AuthenticationFailed { .. } => StatusCode::InvalidCredentials,
            Self::Io { .. } => StatusCode::Io,
            Self::Protocol { .. } => StatusCode::ProtocolError,
            Self::EncodingError { .. } => StatusCode::SerializationFailed,
            Self::DecodingError { .. } => StatusCode::DeserializationFailed,
            Self::IncompleteData => StatusCode::Unexpected,
            Self::ReadTimeout | Self::WriteTimeout => StatusCode::Timeout,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::ConnectionFailed { address, .. } => {
                format!("Failed to connect to {address}")
            }
            Self::ConnectionTimeout => "Connection timeout".to_string(),
            Self::ConnectionClosed => "Connection closed by server".to_string(),
            Self::ServerError { message } => format!("Server error: {message}"),
            Self::InvalidCommand => "Invalid command".to_string(),
            Self::UnknownCommand { command } => format!("Unknown command: {command}"),
            Self::UnexpectedResponse => "Unexpected response from server".to_string(),
            Self::AuthenticationFailed { .. } => "Authentication failed".to_string(),
            Self::Io { .. } => "Network error occurred".to_string(),
            Self::Protocol { .. } => "Protocol error".to_string(),
            Self::EncodingError { .. } => "Failed to encode message".to_string(),
            Self::DecodingError { .. } => "Failed to decode message".to_string(),
            Self::IncompleteData => "Incomplete data received".to_string(),
            Self::ReadTimeout => "Read operation timed out".to_string(),
            Self::WriteTimeout => "Write operation timed out".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "client".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::ConnectionFailed { address, .. } => {
                tags.push(("address", address.clone()));
            }
            Self::UnknownCommand { command } => {
                tags.push(("command", command.clone()));
            }
            Self::Io { kind, .. } => {
                tags.push(("io_kind", format!("{kind:?}")));
            }
            _ => {}
        }

        tags
    }
}

// Конверсия из io::Error
impl From<io::Error> for ClientError {
    fn from(err: io::Error) -> Self {
        Self::Io {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, ErrorKind};

    use super::*;

    /// Тест проверяет статус код, Display и client_message для
    /// ConnectionFailed.
    #[test]
    fn test_connection_failed() {
        let err = ClientError::ConnectionFailed {
            address: "localhost:6379".to_string(),
            reason: "connection refused".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::ConnectionFailed);
        assert!(err.to_string().contains("localhost:6379"));
        assert!(err.client_message().contains("localhost:6379"));
    }

    /// Тест проверяет, что AuthenticationFailed возвращает корректный статус и
    /// не раскрывает детали в client_message.
    #[test]
    fn test_authentication_failed() {
        let err = ClientError::AuthenticationFailed {
            reason: "invalid password".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::InvalidCredentials);
        // Проверяем, что детали не раскрываются в client_message
        assert!(!err.client_message().contains("invalid password"));
    }

    /// Тест проверяет конвертацию io::Error -> ClientError и корректность меток
    /// metrics_tags (io_kind).
    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(ErrorKind::BrokenPipe, "broken pipe");
        let client_err: ClientError = io_err.into();

        assert_eq!(client_err.status_code(), StatusCode::Io);
        let tags = client_err.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| k == &"io_kind" && v.contains("BrokenPipe")));
    }

    /// Тест проверяет Display и статус для ClientError::Protocol.
    #[test]
    fn test_protocol_error() {
        let err = ClientError::Protocol {
            reason: "malformed header".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::ProtocolError);
        assert!(err.to_string().contains("malformed header"));
    }

    /// Тест проверяет статус код для таймаутов чтения и записи.
    #[test]
    fn test_timeout_errors() {
        let read_err = ClientError::ReadTimeout;
        let write_err = ClientError::WriteTimeout;

        assert_eq!(read_err.status_code(), StatusCode::Timeout);
        assert_eq!(write_err.status_code(), StatusCode::Timeout);
    }

    /// Тест проверяет, что metrics_tags содержит адрес для ConnectionFailed.
    #[test]
    fn test_connection_failed_metrics_contains_address() {
        let err = ClientError::ConnectionFailed {
            address: "127.0.0.1:6379".to_string(),
            reason: "no route".to_string(),
        };
        let tags = err.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| *k == "address" && v == "127.0.0.1:6379"));
    }

    /// Тест проверяет metrics_tags и client_message для UnknownCommand.
    #[test]
    fn test_unknown_command_metrics_and_client_message() {
        let err = ClientError::UnknownCommand {
            command: "FOO".to_string(),
        };
        assert_eq!(err.client_message(), "Unknown command: FOO");
        let tags = err.metrics_tags();
        assert!(tags.iter().any(|(k, v)| *k == "command" && v == "FOO"));
    }

    /// Тест проверяет Display и From<io::Error> для ClientError::Io (включая
    /// текст).
    #[test]
    fn test_io_display_and_from_mapping() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let client_err: ClientError = io_err.into();
        // Display должен содержать ErrorKind и сообщение
        assert!(format!("{}", client_err).contains("PermissionDenied"));
        assert!(format!("{}", client_err).contains("denied"));
        // статус код
        assert_eq!(client_err.status_code(), StatusCode::Io);
    }

    /// Тест проверяет as_any() и возможность downcast_ref для ClientError.
    #[test]
    fn test_as_any_downcast_client_error() {
        let err = ClientError::ServerError {
            message: "oops".to_string(),
        };
        let any_ref: &dyn std::any::Any = err.as_any();
        let down = any_ref.downcast_ref::<ClientError>();
        assert!(down.is_some());
        if let Some(ClientError::ServerError { message }) = down {
            assert_eq!(message, "oops");
        } else {
            panic!("Ожидался ClientError::ServerError");
        }
    }

    /// Тест проверяет client_message и статус для EncodingError и
    /// DecodingError.
    #[test]
    fn test_encoding_and_decoding_client_messages() {
        let enc = ClientError::EncodingError {
            reason: "bad".to_string(),
        };
        let dec = ClientError::DecodingError {
            reason: "bad".to_string(),
        };
        assert_eq!(enc.client_message(), "Failed to encode message");
        assert_eq!(dec.client_message(), "Failed to decode message");
        assert_eq!(enc.status_code(), StatusCode::SerializationFailed);
        assert_eq!(dec.status_code(), StatusCode::DeserializationFailed);
    }

    /// Тест проверяет client_message для IncompleteData и время ожидания
    /// (timeouts).
    #[test]
    fn test_incomplete_and_timeout_client_messages() {
        let inc = ClientError::IncompleteData;
        assert_eq!(inc.client_message(), "Incomplete data received");
        let r = ClientError::ReadTimeout;
        let w = ClientError::WriteTimeout;
        assert_eq!(r.client_message(), "Read operation timed out");
        assert_eq!(w.client_message(), "Write operation timed out");
    }
}
