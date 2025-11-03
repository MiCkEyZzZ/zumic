use crate::{ErrorExt, StatusCode};

/// Ошибка аутентификации.
#[derive(Debug, Clone)]
pub enum AuthError {
    /// Неверные учётные данные
    InvalidCredentials { username: String },
    /// Пользователь не найден
    UserNotFound { username: String },
    /// Пользователь уже существует
    UserExists { username: String },
    /// Ошибка хеширования пароля
    PasswordHashFailed { reason: String },
    /// Ошибка верификации пароля
    PasswordVerifyFailed,
    /// Токен истёк
    TokenExpired { token_id: String },
    /// Невалидный токен
    InvalidToken { reason: String },
    /// Сессия истекла
    SessionExpired { session_id: String },
    /// Невалидная сессия
    InvalidSession { session_id: String },
    /// Доступ запрещён
    PermissionDenied { resource: String, action: String },
    /// Слишком много попыток входа
    TooManyAttempts { username: String, retry_after: u64 },
    /// Доступ к каналу запрещён
    ChannelAccessDenied { channel: String, username: String },
    /// Невалидное ACL правило
    InvalidAclRule { rule: String, reason: String },
    /// Ошибка сериализации ACL
    AclSerializationFailed { reason: String },
}

impl std::fmt::Display for AuthError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::InvalidCredentials { .. } => write!(f, "Invalid credentials"),
            Self::UserNotFound { username } => write!(f, "User not found: {username}"),
            Self::UserExists { username } => write!(f, "User already exists: {username}"),
            Self::PasswordHashFailed { reason } => {
                write!(f, "Password hashing failed: {reason}")
            }
            Self::PasswordVerifyFailed => write!(f, "Password verification failed"),
            Self::TokenExpired { token_id } => write!(f, "Token expired: {token_id}"),
            Self::InvalidToken { reason } => write!(f, "Invalid token: {reason}"),
            Self::SessionExpired { session_id } => write!(f, "Session expired: {session_id}"),
            Self::InvalidSession { session_id } => write!(f, "Invalid session: {session_id}"),
            Self::PermissionDenied { resource, action } => {
                write!(f, "Permission denied: cannot {action} {resource}")
            }
            Self::TooManyAttempts {
                username,
                retry_after,
            } => {
                write!(
                    f,
                    "Too many authentication attempts for user {username}. Retry after {retry_after}s"
                )
            }
            Self::ChannelAccessDenied { channel, username } => {
                write!(f, "User {username} denied access to channel {channel}")
            }
            Self::InvalidAclRule { rule, reason } => {
                write!(f, "Invalid ACL rule '{rule}': {reason}")
            }
            Self::AclSerializationFailed { reason } => {
                write!(f, "ACL serialization failed: {reason}")
            }
        }
    }
}

impl std::error::Error for AuthError {}

impl ErrorExt for AuthError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidCredentials { .. } | Self::PasswordVerifyFailed => {
                StatusCode::InvalidCredentials
            }
            Self::UserNotFound { .. } => StatusCode::UserNotFound,
            Self::UserExists { .. } => StatusCode::UserExists,
            Self::PasswordHashFailed { .. } => StatusCode::PasswordHashFailed,
            Self::TokenExpired { .. } | Self::SessionExpired { .. } => StatusCode::SessionExpired,
            Self::InvalidToken { .. } | Self::InvalidSession { .. } => StatusCode::InvalidToken,
            Self::PermissionDenied { .. } | Self::ChannelAccessDenied { .. } => {
                StatusCode::PermissionDenied
            }
            Self::TooManyAttempts { .. } => StatusCode::TooManyAttempts,
            Self::InvalidAclRule { .. } => StatusCode::InvalidArgs,
            Self::AclSerializationFailed { .. } => StatusCode::SerializationFailed,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            // Скрываем имя пользователя в некоторых случаях для безопасности
            Self::InvalidCredentials { .. } => "Invalid username or password".to_string(),
            Self::UserNotFound { .. } => "Authentication failed".to_string(),
            Self::PasswordHashFailed { .. } | Self::AclSerializationFailed { .. } => {
                "Internal server error".to_string()
            }
            Self::PasswordVerifyFailed => "Authentication failed".to_string(),
            Self::TokenExpired { .. } => "Token has expired".to_string(),
            Self::InvalidToken { .. } => "Invalid authentication token".to_string(),
            Self::SessionExpired { .. } => "Session has expired".to_string(),
            Self::InvalidSession { .. } => "Invalid session".to_string(),
            Self::PermissionDenied { resource, action } => {
                format!("Access denied: cannot {action} {resource}")
            }
            Self::TooManyAttempts { retry_after, .. } => {
                format!(
                    "Too many authentication attempts. Please retry after {retry_after} seconds"
                )
            }
            Self::ChannelAccessDenied { channel, .. } => {
                format!("Access denied to channel: {channel}")
            }
            Self::InvalidAclRule { reason, .. } => format!("Invalid ACL rule: {reason}"),
            Self::UserExists { .. } => "User already exists".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "auth".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::PermissionDenied { resource, action } => {
                tags.push(("resource", resource.clone()));
                tags.push(("action", action.clone()));
            }
            Self::ChannelAccessDenied { channel, username } => {
                tags.push(("username", username.clone()));
                tags.push(("channel", channel.clone()));
            }
            Self::InvalidCredentials { username }
            | Self::UserNotFound { username }
            | Self::UserExists { username }
            | Self::TooManyAttempts { username, .. } => {
                tags.push(("username", username.clone()));
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
    fn test_invalid_credentials() {
        let err = AuthError::InvalidCredentials {
            username: "admin".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::InvalidCredentials);
        // Проверяем, что username не раскрывается в client_message
        assert!(!err.client_message().contains("admin"));
        assert!(err.to_string().contains("Invalid credentials"));
    }

    #[test]
    fn test_permission_denied() {
        let err = AuthError::PermissionDenied {
            resource: "database:users".to_string(),
            action: "delete".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::PermissionDenied);

        let tags = err.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| k == &"resource" && v == "database:users"));
        assert!(tags.iter().any(|(k, v)| k == &"action" && v == "delete"));
    }

    #[test]
    fn test_too_many_attempts() {
        let err = AuthError::TooManyAttempts {
            username: "test".to_string(),
            retry_after: 60,
        };
        assert_eq!(err.status_code(), StatusCode::TooManyAttempts);
        assert!(err.client_message().contains("60 seconds"));
    }
}
