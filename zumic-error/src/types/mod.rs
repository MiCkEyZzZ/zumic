pub mod auth;
pub mod client;
pub mod cluster;
pub mod memory;
pub mod network;
pub mod persistent;
pub mod pubsub;
pub mod storage;
pub mod zdb_error;
pub mod zsp_error;

// Публичный экспорт всех типов ошибок и функций из вложенных
// модулей, чтобы упростить доступ к ним из внешнего кода.
pub use auth::*;
pub use client::*;
pub use cluster::*;
pub use memory::*;
pub use network::*;
pub use persistent::*;
pub use pubsub::*;
pub use storage::*;
pub use zdb_error::*;
pub use zsp_error::*;

use crate::{ErrorExt, StatusCode};

/// Универсальная ошибка с кодом и сообщением.
#[derive(Debug, Clone)]
pub struct GenericError {
    code: StatusCode,
    message: String,
}

impl GenericError {
    pub fn new(
        code: StatusCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for GenericError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GenericError {}

impl ErrorExt for GenericError {
    fn status_code(&self) -> StatusCode {
        self.code
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Конвертация из std::io::Error
impl From<std::io::Error> for crate::StackError {
    fn from(err: std::io::Error) -> Self {
        let code = match err.kind() {
            std::io::ErrorKind::NotFound => StatusCode::NotFound,
            std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
            std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted => StatusCode::ConnectionFailed,
            std::io::ErrorKind::TimedOut => StatusCode::Timeout,
            std::io::ErrorKind::UnexpectedEof => StatusCode::UnexpectedEof,
            _ => StatusCode::Io,
        };

        crate::StackError::new(GenericError::new(code, err.to_string()))
    }
}

/// Конвертация из std::str::Utf8Error
impl From<std::str::Utf8Error> for crate::StackError {
    fn from(err: std::str::Utf8Error) -> Self {
        crate::StackError::new(GenericError::new(
            StatusCode::InvalidUtf8,
            format!("UTF-8 decoding failed: {err}"),
        ))
    }
}

/// Конвертация из std::string::FromUtf8Error
impl From<std::string::FromUtf8Error> for crate::StackError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        crate::StackError::new(GenericError::new(
            StatusCode::InvalidUtf8,
            format!("UTF-8 conversion failed: {err}"),
        ))
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn test_generic_error() {
        let err = GenericError::new(StatusCode::InvalidArgs, "test message");
        assert_eq!(err.status_code(), StatusCode::InvalidArgs);
        assert_eq!(err.to_string(), "test message");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let stack: crate::StackError = io_err.into();
        assert_eq!(stack.status_code(), StatusCode::NotFound);
    }

    /// Тест проверяет базовое поведение GenericError: статус код и вывод в
    /// строку.
    #[test]
    fn test_generic_error_basic() {
        let err = GenericError::new(StatusCode::InvalidArgs, "test message");
        assert_eq!(err.status_code(), StatusCode::InvalidArgs);
        assert_eq!(err.to_string(), "test message");
    }

    /// Тест проверяет, что GenericError реализует std::error::Error
    /// (компиляционно).
    #[test]
    fn test_generic_error_impls_error() {
        let err = GenericError::new(StatusCode::InvalidArgs, "ok");
        let _err_ref: &dyn std::error::Error = &err;
        // Если собирается — значит impl std::error::Error присутствует.
    }

    /// Тест проверяет, что as_any() позволяет сделать downcast_ref к
    /// GenericError.
    #[test]
    fn test_generic_error_as_any_downcast() {
        let err = GenericError::new(StatusCode::NotFound, "not found");
        let any_ref: &dyn std::any::Any = err.as_any();
        let down = any_ref.downcast_ref::<GenericError>();
        assert!(down.is_some());
        let down = down.unwrap();
        assert_eq!(down.status_code(), StatusCode::NotFound);
        assert_eq!(down.to_string(), "not found");
    }

    /// Тест проверяет маппинг std::io::ErrorKind -> StatusCode в
    /// From<std::io::Error> for StackError.
    #[test]
    fn test_io_error_kind_mapping() {
        let cases = vec![
            (
                io::ErrorKind::NotFound,
                StatusCode::NotFound,
                "file not found",
            ),
            (
                io::ErrorKind::PermissionDenied,
                StatusCode::PermissionDenied,
                "perm",
            ),
            (
                io::ErrorKind::ConnectionRefused,
                StatusCode::ConnectionFailed,
                "refused",
            ),
            (io::ErrorKind::TimedOut, StatusCode::Timeout, "timeout"),
            (
                io::ErrorKind::UnexpectedEof,
                StatusCode::UnexpectedEof,
                "eof",
            ),
            (io::ErrorKind::Other, StatusCode::Io, "other"),
        ];

        for (kind, expected_code, msg) in cases {
            let io_err = io::Error::new(kind, msg);
            let stack: crate::StackError = io_err.into();
            assert_eq!(stack.status_code(), expected_code, "kind={:?}", kind);
            // убеждаемся, что сообщение не теряется
            assert!(stack.to_string().contains(msg));
        }
    }

    /// Тест проверяет конвертацию std::str::Utf8Error -> StackError и
    /// корректность кода/сообщения.
    #[test]
    fn test_from_utf8_error_conversion() {
        // формируем байты в рантайме, чтобы не вызывать lint о "invalid literal"
        let bytes = vec![0xff];
        let bad = std::str::from_utf8(&bytes).unwrap_err();
        let stack: crate::StackError = bad.into();
        assert_eq!(stack.status_code(), StatusCode::InvalidUtf8);
        assert!(stack.to_string().contains("UTF-8 decoding failed"));
    }

    /// Тест проверяет конвертацию std::string::FromUtf8Error -> StackError и
    /// корректность кода/сообщения.
    #[test]
    fn test_from_fromutf8error_conversion() {
        let err = String::from_utf8(vec![0xff]).unwrap_err();
        let stack: crate::StackError = err.into();
        assert_eq!(stack.status_code(), StatusCode::InvalidUtf8);
        assert!(stack.to_string().contains("UTF-8 conversion failed"));
    }
}
