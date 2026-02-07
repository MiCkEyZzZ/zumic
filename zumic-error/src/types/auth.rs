use std::fmt;

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
    /// Невалидный ключ
    InvalidKey { reason: String },
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
    /// Ошибка авторизации
    SigningFailed { reason: String },
    /// Токен отозван
    Revoked { reason: String },
}

#[derive(Debug, Clone)]
pub enum SessionError {
    NotFound,
    Expired,
    IpMismatch,
    UserAgentMismatch,
    TokenExpired,
    TokenRevoked,
    InvalidSessionId,
    Storage(String),
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для AuthError, SessionError
////////////////////////////////////////////////////////////////////////////////

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
            Self::InvalidKey { reason } => write!(f, "Invalid key: {reason}"),
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
            Self::SigningFailed { reason } => {
                write!(f, "ACL serialization failed: {reason}")
            }
            Self::Revoked { reason } => {
                write!(f, "ACL serialization failed: {reason}")
            }
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            SessionError::NotFound => write!(f, "Session not found"),
            SessionError::Expired => write!(f, "Session expired"),
            SessionError::IpMismatch => write!(f, "IP address mismatch"),
            SessionError::UserAgentMismatch => write!(f, "User-Agent mismatch"),
            SessionError::TokenExpired => write!(f, "JWT token expired"),
            SessionError::TokenRevoked => write!(f, "JWT token revoked"),
            SessionError::InvalidSessionId => write!(f, "Invalid session ID"),
            SessionError::Storage(msg) => write!(f, "Storage error: {msg}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl std::error::Error for SessionError {}

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
            Self::InvalidKey { .. } => StatusCode::InvalidKey,
            Self::PermissionDenied { .. } | Self::ChannelAccessDenied { .. } => {
                StatusCode::PermissionDenied
            }
            Self::TooManyAttempts { .. } => StatusCode::TooManyAttempts,
            Self::InvalidAclRule { .. } => StatusCode::InvalidArgs,
            Self::AclSerializationFailed { .. } => StatusCode::SerializationFailed,
            Self::SigningFailed { .. } => StatusCode::InvalidArgs,
            Self::Revoked { .. } => StatusCode::InvalidArgs,
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
            Self::InvalidKey { .. } => "Invalid authentication key".to_string(),
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
            Self::SigningFailed { .. } => "User already exists".to_string(),
            Self::Revoked { .. } => "User already exists".to_string(),
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

impl ErrorExt for SessionError {
    fn status_code(&self) -> StatusCode {
        match self {
            SessionError::NotFound => StatusCode::NotFound,
            SessionError::Expired | SessionError::TokenExpired => StatusCode::SessionExpired,
            SessionError::IpMismatch | SessionError::UserAgentMismatch => {
                StatusCode::PermissionDenied
            }
            SessionError::TokenRevoked => StatusCode::Unauthorized,
            SessionError::InvalidSessionId => StatusCode::InvalidToken,
            SessionError::Storage(_) => StatusCode::Internal,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            SessionError::NotFound | SessionError::InvalidSessionId => {
                "Invalid session".to_string()
            }
            SessionError::Expired => "Session has expired".to_string(),
            SessionError::TokenExpired => "Authentication token has expired".to_string(),
            SessionError::TokenRevoked => "Authentication token has been revoked".to_string(),
            SessionError::IpMismatch => "Session IP mismatch".to_string(),
            SessionError::UserAgentMismatch => "Session fingerprint mismatch".to_string(),
            SessionError::Storage(_) => "Internal server error".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        vec![
            ("error_type", "session".to_string()),
            ("status_code", self.status_code().to_string()),
        ]
    }
}

impl From<SessionError> for AuthError {
    fn from(err: SessionError) -> Self {
        match err {
            SessionError::NotFound => AuthError::InvalidSession {
                session_id: "<unknown>".into(),
            },
            SessionError::Expired => AuthError::SessionExpired {
                session_id: "<expired>".into(),
            },
            SessionError::TokenExpired => AuthError::TokenExpired {
                // ← НОВЫЙ
                token_id: "<expired>".into(),
            },
            SessionError::TokenRevoked => AuthError::Revoked {
                // ← НОВЫЙ
                reason: "Token has been revoked".into(),
            },
            SessionError::IpMismatch | SessionError::UserAgentMismatch => {
                AuthError::PermissionDenied {
                    resource: "session".into(),
                    action: "use".into(),
                }
            }
            SessionError::InvalidSessionId => AuthError::InvalidSession {
                session_id: "<invalid>".into(),
            },
            SessionError::Storage(reason) => AuthError::SigningFailed {
                reason: format!("session storage error: {reason}"),
            },
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

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

    #[test]
    fn test_user_not_found_and_exists() {
        let not_found = AuthError::UserNotFound {
            username: "none".to_string(),
        };
        assert_eq!(not_found.status_code(), StatusCode::UserNotFound);
        assert_eq!(
            not_found.client_message(),
            "Authentication failed".to_string()
        );
        assert!(not_found.to_string().contains("User not found"));

        let exists = AuthError::UserExists {
            username: "exists".to_string(),
        };
        assert_eq!(exists.status_code(), StatusCode::UserExists);
        assert_eq!(exists.client_message(), "User already exists".to_string());
        assert!(exists.to_string().contains("User already exists"));
    }

    #[test]
    fn test_password_errors() {
        let hash_failed = AuthError::PasswordHashFailed {
            reason: "bcrypt failed".to_string(),
        };
        assert_eq!(hash_failed.status_code(), StatusCode::PasswordHashFailed);
        assert_eq!(
            hash_failed.client_message(),
            "Internal server error".to_string()
        );
        assert!(hash_failed.to_string().contains("Password hashing failed"));

        let verify_failed = AuthError::PasswordVerifyFailed;
        assert_eq!(verify_failed.status_code(), StatusCode::InvalidCredentials);
        assert_eq!(
            verify_failed.client_message(),
            "Authentication failed".to_string()
        );
    }

    #[test]
    fn test_token_and_session() {
        let texp = AuthError::TokenExpired {
            token_id: "tok1".to_string(),
        };
        assert_eq!(texp.status_code(), StatusCode::SessionExpired);
        assert_eq!(texp.client_message(), "Token has expired".to_string());
        assert!(texp.to_string().contains("Token expired"));

        let inval_tok = AuthError::InvalidToken {
            reason: "bad sig".to_string(),
        };
        assert_eq!(inval_tok.status_code(), StatusCode::InvalidToken);
        assert_eq!(
            inval_tok.client_message(),
            "Invalid authentication token".to_string()
        );

        let s_exp = AuthError::SessionExpired {
            session_id: "sess1".to_string(),
        };
        assert_eq!(s_exp.status_code(), StatusCode::SessionExpired);
        assert!(s_exp.to_string().contains("Session expired"));

        let inval_sess = AuthError::InvalidSession {
            session_id: "sess2".to_string(),
        };
        assert_eq!(inval_sess.status_code(), StatusCode::InvalidToken);
    }

    #[test]
    fn test_channel_access_and_acl() {
        let ch = AuthError::ChannelAccessDenied {
            channel: "chan42".to_string(),
            username: "bob".to_string(),
        };
        assert_eq!(ch.status_code(), StatusCode::PermissionDenied);
        let tags = ch.metrics_tags();
        assert!(tags.iter().any(|(k, v)| k == &"username" && v == "bob"));
        assert!(tags.iter().any(|(k, v)| k == &"channel" && v == "chan42"));
        assert_eq!(
            ch.client_message(),
            "Access denied to channel: chan42".to_string()
        );

        let invalid_acl = AuthError::InvalidAclRule {
            rule: "foo".to_string(),
            reason: "bad format".to_string(),
        };
        assert_eq!(invalid_acl.status_code(), StatusCode::InvalidArgs);
        assert!(invalid_acl.client_message().contains("Invalid ACL rule"));
        // metrics_tags should still include status_code tag
        let tags2 = invalid_acl.metrics_tags();
        assert!(tags2.iter().any(|(k, _)| k == &"status_code"));
    }

    #[test]
    fn test_acl_serialization() {
        let ser_err = AuthError::AclSerializationFailed {
            reason: "json error".to_string(),
        };
        assert_eq!(ser_err.status_code(), StatusCode::SerializationFailed);
        assert_eq!(
            ser_err.client_message(),
            "Internal server error".to_string()
        );
    }

    #[test]
    fn test_too_many_attempts_tags_and_message() {
        let tma = AuthError::TooManyAttempts {
            username: "eve".to_string(),
            retry_after: 120,
        };
        assert_eq!(tma.status_code(), StatusCode::TooManyAttempts);
        let cm = tma.client_message();
        assert!(cm.contains("120"));
        let tags = tma.metrics_tags();
        assert!(tags.iter().any(|(k, v)| k == &"username" && v == "eve"));
    }

    #[test]
    fn test_as_any_downcast() {
        let err = AuthError::UserNotFound {
            username: "noone".to_string(),
        };
        let any_ref = err.as_any();
        // Убедимся, что можно сделать downcast_ref к AuthError
        assert!(any_ref.downcast_ref::<AuthError>().is_some());
    }

    #[test]
    fn test_session_not_found() {
        let err = SessionError::NotFound;
        assert_eq!(err.status_code(), StatusCode::NotFound);
        assert_eq!(err.client_message(), "Invalid session");
        assert!(err.to_string().contains("Session not found"));
    }

    #[test]
    fn test_session_expired() {
        let err = SessionError::Expired;
        assert_eq!(err.status_code(), StatusCode::SessionExpired);
        assert_eq!(err.client_message(), "Session has expired");
    }

    #[test]
    fn test_session_ip_mismatch() {
        let err = SessionError::IpMismatch;
        assert_eq!(err.status_code(), StatusCode::PermissionDenied);
        assert!(err.client_message().contains("IP"));
    }

    #[test]
    fn test_session_storage_error_hidden() {
        let err = SessionError::Storage("db down".into());
        assert_eq!(err.status_code(), StatusCode::Internal);
        assert_eq!(err.client_message(), "Internal server error");
        assert!(err.to_string().contains("db down"));
    }

    #[test]
    fn test_session_as_any() {
        let err = SessionError::InvalidSessionId;
        let any = err.as_any();
        assert!(any.downcast_ref::<SessionError>().is_some());
    }

    #[test]
    fn test_session_token_expired() {
        let err = SessionError::TokenExpired;
        assert_eq!(err.status_code(), StatusCode::SessionExpired);
        assert!(err.client_message().contains("Authentication token"));
        assert!(err.to_string().contains("JWT token expired"));
    }

    #[test]
    fn test_session_token_revoked() {
        let err = SessionError::TokenRevoked;
        assert_eq!(err.status_code(), StatusCode::Unauthorized);
        assert!(err.client_message().contains("revoked"));
        assert!(err.to_string().contains("JWT token revoked"));
    }

    #[test]
    fn test_session_error_to_auth_error() {
        // TokenExpired -> AuthError::TokenExpired
        let session_err = SessionError::TokenExpired;
        let auth_err: AuthError = session_err.into();
        assert!(matches!(auth_err, AuthError::TokenExpired { .. }));

        // TokenRevoked -> AuthError::Revoked
        let session_err = SessionError::TokenRevoked;
        let auth_err: AuthError = session_err.into();
        assert!(matches!(auth_err, AuthError::Revoked { .. }));
    }
}
