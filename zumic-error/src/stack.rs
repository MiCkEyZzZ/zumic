use std::{fmt, panic::Location, sync::Arc};

#[cfg(feature = "serde")]
use serde::Serialize;

use crate::{ErrorExt, LogLevel, StatusCode};

/// Основная структура ошибки с поддержкой контекста и трассировки.
///
/// Позволяет добавлять контекстную информацию по мере распространения ошибки
/// вверх по стеку вызовов.
#[derive(Clone)]
pub struct StackError {
    inner: Arc<dyn ErrorExt>,
    contexts: Arc<Vec<ErrorContext>>,
}

/// Контекст ошибки с location tracking.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub message: String,
    pub location: Option<&'static Location<'static>>,
}

/// Структура для сериализации ошибок в API ответах.
#[cfg(feature = "serde")]
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: u32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<String>>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl StackError {
    /// Создаёт новую ошибку.
    #[track_caller]
    pub fn new<E: ErrorExt>(err: E) -> Self {
        Self {
            inner: Arc::new(err),
            contexts: Arc::new(Vec::new()),
        }
    }

    /// Добавляет контекст к ошибке.
    #[track_caller]
    pub fn context(
        mut self,
        msg: impl Into<String>,
    ) -> Self {
        let mut new_contexts = (*self.contexts).clone();
        new_contexts.push(ErrorContext {
            message: msg.into(),
            location: Some(Location::caller()),
        });
        self.contexts = Arc::new(new_contexts);
        self
    }

    /// Возвращает код статуса
    pub fn status_code(&self) -> StatusCode {
        self.inner.status_code()
    }

    /// Возвращает сообщение для клиента.
    pub fn client_message(&self) -> String {
        self.inner.client_message()
    }

    /// Возвращает корневую ошибку.
    pub fn root(&self) -> &dyn ErrorExt {
        self.inner.as_ref()
    }

    /// Возвращает все контексты
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }

    /// Получить метрики/теги
    pub fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        self.inner.metrics_tags()
    }

    /// Попытка downcast к конкретному типу ошибки
    pub fn downcast_ref<T: ErrorExt + 'static>(&self) -> Option<&T> {
        self.inner.as_any().downcast_ref::<T>()
    }

    /// Сериализация для API ответов (требует feature = "serde")
    #[cfg(feature = "serde")]
    pub fn to_response(&self) -> ErrorResponse {
        ErrorResponse {
            code: self.status_code().code(),
            message: self.client_message(),
            #[cfg(debug_assertions)]
            contexts: Some(self.format_contexts()),
            #[cfg(not(debug_assertions))]
            contexts: None,
        }
    }

    /// Форматировать контексты для вывода
    fn format_contexts(&self) -> Vec<String> {
        self.contexts
            .iter()
            .map(|ctx| {
                if let Some(loc) = ctx.location {
                    format!("{} ({}:{})", ctx.message, loc.file(), loc.line())
                } else {
                    ctx.message.clone()
                }
            })
            .collect()
    }

    /// Возвращает уровень логирования.
    pub fn log_level(&self) -> LogLevel {
        self.status_code().log_level()
    }

    /// Проверяет, является ли ошибка критичной.
    pub fn is_critical(&self) -> bool {
        self.status_code().is_critical()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для StackError
////////////////////////////////////////////////////////////////////////////////

impl fmt::Debug for StackError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        let mut debug = f.debug_struct("StackError");
        debug.field("inner", &self.inner.to_string());
        debug.field("status_code", &self.status_code());

        if !self.contexts.is_empty() {
            debug.field("contexts", &self.format_contexts());
        }

        debug.finish()
    }
}

impl fmt::Display for StackError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        if !self.contexts.is_empty() {
            let contexts: Vec<&str> = self.contexts.iter().map(|c| c.message.as_str()).collect();
            write!(f, "{}: {}", contexts.join(" → "), self.inner)
        } else {
            write!(f, "{}", self.inner)
        }
    }
}

impl std::error::Error for StackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.inner.as_ref())
    }
}

impl<E: ErrorExt> From<E> for StackError {
    #[track_caller]
    fn from(e: E) -> Self {
        StackError::new(e)
    }
}

impl From<StackError> for std::io::Error {
    fn from(e: StackError) -> Self {
        std::io::Error::other(e.to_string())
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthError;

    #[test]
    fn test_context_chain() {
        let err = AuthError::InvalidCredentials {
            username: "test".to_string(),
        };
        let stack = StackError::new(err)
            .context("Authentication failed")
            .context("Login handler");

        assert_eq!(stack.contexts().len(), 2);
        assert_eq!(stack.contexts()[0].message, "Authentication failed");
        assert!(stack.contexts()[0].location.is_some());
    }

    #[test]
    fn test_downcast() {
        let err = AuthError::UserNotFound {
            username: "admin".to_string(),
        };
        let stack = StackError::new(err);

        let downcasted = stack.downcast_ref::<AuthError>();
        assert!(downcasted.is_some());
    }

    #[test]
    fn test_display() {
        let err = AuthError::TooManyAttempts {
            username: "test".to_string(),
            retry_after: 60,
        };
        let stack = StackError::new(err).context("Rate limit check");

        let display = stack.to_string();
        assert!(display.contains("Rate limit check"));
        assert!(display.contains("Too many authentication attempts"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_serialization() {
        let err = AuthError::SessionExpired {
            session_id: "abc123".to_string(),
        };
        let stack = StackError::new(err).context("Session validation");

        let response = stack.to_response();
        assert_eq!(response.code, StatusCode::SessionExpired as u32);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Session expired"));
    }
}
