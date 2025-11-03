use std::any::Any;

use crate::{ErrorExt, StatusCode};

/// Ошибки получения сообщений (блокирующие операции)
#[derive(Debug, Clone)]
pub enum RecvError {
    /// Канал закрыт
    Closed,
    /// Таймаут операции
    Timeout,
    /// Ошибка сериализации/десериализации сообщения
    SerializationError { reason: String },
    /// Получатель отстал (пропущены сообщения)
    Lagged { count: u64 },
    /// Невалидный glob паттерн для подписки
    InvalidPattern { pattern: String, reason: String },
    /// Канал не найден
    ChannelNotFound { channel: String },
    /// Превышен лимит подписчиков
    SubscriberLimitExceeded { channel: String, limit: usize },
}

/// Ошибки неблокирующего получения сообщений
#[derive(Debug, Clone)]
pub enum TryRecvError {
    /// Нет доступных сообщений
    Empty,
    /// Канал закрыт
    Closed,
    /// Получатель отстал
    Lagged { count: u64 },
}

/// Ошибки публикации сообщений
#[derive(Debug, Clone)]
pub enum PublishError {
    /// Канал закрыт
    ChannelClosed { channel: String },

    /// Превышен лимит размера сообщения
    MessageTooLarge { size: usize, max: usize },

    /// Ошибка сериализации
    SerializationFailed { reason: String },

    /// Нет подписчиков
    NoSubscribers { channel: String },

    /// Ошибка доставки
    DeliveryFailed { channel: String, reason: String },
}

impl std::fmt::Display for RecvError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "Channel is closed"),
            Self::Timeout => write!(f, "Operation exceeded the specified timeout"),
            Self::SerializationError { reason } => {
                write!(f, "Message serialization/deserialization error: {reason}")
            }
            Self::Lagged { count } => write!(f, "Receiver lagged behind by {count} messages"),
            Self::InvalidPattern { pattern, reason } => {
                write!(f, "Invalid glob pattern '{pattern}': {reason}")
            }
            Self::ChannelNotFound { channel } => {
                write!(f, "Channel not found: {channel}")
            }
            Self::SubscriberLimitExceeded { channel, limit } => {
                write!(
                    f,
                    "Subscriber limit ({limit}) exceeded for channel {channel}"
                )
            }
        }
    }
}

impl std::error::Error for RecvError {}

impl ErrorExt for RecvError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Closed => StatusCode::ConnectionClosed,
            Self::Timeout => StatusCode::Timeout,
            Self::SerializationError { .. } => StatusCode::SerializationFailed,
            Self::Lagged { .. } => StatusCode::RateLimited,
            Self::InvalidPattern { .. } => StatusCode::InvalidArgs,
            Self::ChannelNotFound { .. } => StatusCode::NotFound,
            Self::SubscriberLimitExceeded { .. } => StatusCode::SubscriberLimitExceeded,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::Closed => "Channel closed".to_string(),
            Self::Timeout => "Operation timeout".to_string(),
            Self::SerializationError { .. } => "Message format error".to_string(),
            Self::Lagged { count } => format!("Lagged behind by {count} messages"),
            Self::InvalidPattern { .. } => "Invalid subscription pattern".to_string(),
            Self::ChannelNotFound { channel } => format!("Channel not found: {channel}"),
            Self::SubscriberLimitExceeded { .. } => "Too many subscribers".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "pubsub_recv".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::Lagged { count } => {
                tags.push(("lagged_count", count.to_string()));
            }
            Self::ChannelNotFound { channel } | Self::SubscriberLimitExceeded { channel, .. } => {
                tags.push(("channel", channel.clone()));
            }
            _ => {}
        }

        tags
    }
}

impl std::fmt::Display for TryRecvError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "No messages available"),
            Self::Closed => write!(f, "Channel is closed"),
            Self::Lagged { count } => write!(f, "Receiver lagged behind by {count} messages"),
        }
    }
}

impl std::error::Error for TryRecvError {}

impl ErrorExt for TryRecvError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Empty => StatusCode::NotFound,
            Self::Closed => StatusCode::ConnectionClosed,
            Self::Lagged { .. } => StatusCode::RateLimited,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::Empty => "No messages available".to_string(),
            Self::Closed => "Channel closed".to_string(),
            Self::Lagged { count } => format!("Lagged behind by {count} messages"),
        }
    }
}

impl std::fmt::Display for PublishError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::ChannelClosed { channel } => write!(f, "Channel closed: {channel}"),
            Self::MessageTooLarge { size, max } => {
                write!(f, "Message too large: {size} bytes (max {max})")
            }
            Self::SerializationFailed { reason } => {
                write!(f, "Message serialization failed: {reason}")
            }
            Self::NoSubscribers { channel } => write!(f, "No subscribers for channel: {channel}"),
            Self::DeliveryFailed { channel, reason } => {
                write!(f, "Message delivery failed for {channel}: {reason}")
            }
        }
    }
}

impl std::error::Error for PublishError {}

impl ErrorExt for PublishError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::ChannelClosed { .. } => StatusCode::ConnectionClosed,
            Self::MessageTooLarge { .. } => StatusCode::SizeLimit,
            Self::SerializationFailed { .. } => StatusCode::SerializationFailed,
            Self::NoSubscribers { .. } => StatusCode::NotFound,
            Self::DeliveryFailed { .. } => StatusCode::Internal,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::ChannelClosed { .. } => "Channel closed".to_string(),
            Self::MessageTooLarge { max, .. } => {
                format!("Message size exceeds limit ({max} bytes)")
            }
            Self::SerializationFailed { .. } => "Message format error".to_string(),
            Self::NoSubscribers { .. } => "No subscribers".to_string(),
            Self::DeliveryFailed { .. } => "Message delivery failed".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "pubsub_publish".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::ChannelClosed { channel }
            | Self::NoSubscribers { channel }
            | Self::DeliveryFailed { channel, .. } => {
                tags.push(("channel", channel.clone()));
            }
            Self::MessageTooLarge { size, max } => {
                tags.push(("message_size", size.to_string()));
                tags.push(("max_size", max.to_string()));
            }
            _ => {}
        }

        tags
    }
}

/// Конвертация из tokio::sync::broadcast::error::RecvError
#[cfg(feature = "tokio")]
impl From<tokio::sync::broadcast::error::RecvError> for RecvError {
    fn from(err: tokio::sync::broadcast::error::RecvError) -> Self {
        match err {
            tokio::sync::broadcast::error::RecvError::Closed => RecvError::Closed,
            tokio::sync::broadcast::error::RecvError::Lagged(n) => RecvError::Lagged { count: n },
        }
    }
}

#[cfg(feature = "tokio")]
impl From<tokio::sync::broadcast::error::TryRecvError> for TryRecvError {
    fn from(err: tokio::sync::broadcast::error::TryRecvError) -> Self {
        match err {
            tokio::sync::broadcast::error::TryRecvError::Empty => TryRecvError::Empty,
            tokio::sync::broadcast::error::TryRecvError::Closed => TryRecvError::Closed,
            tokio::sync::broadcast::error::TryRecvError::Lagged(n) => {
                TryRecvError::Lagged { count: n }
            }
        }
    }
}

/// Конвертация из globset::Error
#[cfg(feature = "globset")]
impl From<globset::Error> for RecvError {
    fn from(err: globset::Error) -> Self {
        RecvError::InvalidPattern {
            pattern: String::new(),
            reason: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recv_error() {
        let err = RecvError::Lagged { count: 42 };
        assert_eq!(err.status_code(), StatusCode::RateLimited);
        assert!(err.to_string().contains("42 messages"));
    }

    #[test]
    fn test_try_recv_error() {
        let err = TryRecvError::Empty;
        assert_eq!(err.status_code(), StatusCode::NotFound);
    }

    #[test]
    fn test_publish_error() {
        let err = PublishError::MessageTooLarge {
            size: 2048,
            max: 1024,
        };
        assert_eq!(err.status_code(), StatusCode::SizeLimit);

        let tags = err.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| k == &"message_size" && v == "2048"));
        assert!(tags.iter().any(|(k, v)| k == &"max_size" && v == "1024"));
    }

    #[test]
    fn test_subscriber_limit() {
        let err = RecvError::SubscriberLimitExceeded {
            channel: "news".to_string(),
            limit: 1000,
        };
        assert_eq!(err.status_code(), StatusCode::SubscriberLimitExceeded);
    }
}
