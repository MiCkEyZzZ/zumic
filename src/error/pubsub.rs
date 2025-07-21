use thiserror::Error;
use tokio::sync::broadcast;

/// Ошибка при получении сообщений.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecvError {
    #[error("channel is closed")]
    Closed,

    #[error("operation exceeded the specified timeout")]
    Timeout,

    #[error("message serialization/deserialization error")]
    SerializationError(String),

    #[error("receiver lagged behind by {0} messages")]
    Lagged(u64),

    #[error("invalid glob pattern for subscription")]
    InvalidPattern(String),

    #[error("channel not found (attempt to operate on a non-existent channel)")]
    ChannelNotFound(String),

    #[error("channel subscriber limit exceeded")]
    SubscriberLimitExceeded,
}

/// Ошибка при неблокирующем получении сообщений.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TryRecvError {
    #[error("no messages available")]
    Empty,

    #[error("channel is closed")]
    Closed,

    #[error("receiver lagged behind by {0} messages")]
    Lagged(u64),
}

// === Преобразования ===

impl From<broadcast::error::RecvError> for RecvError {
    fn from(err: broadcast::error::RecvError) -> Self {
        match err {
            broadcast::error::RecvError::Closed => RecvError::Closed,
            broadcast::error::RecvError::Lagged(n) => RecvError::Lagged(n),
        }
    }
}

impl From<broadcast::error::TryRecvError> for TryRecvError {
    fn from(err: broadcast::error::TryRecvError) -> Self {
        match err {
            broadcast::error::TryRecvError::Empty => TryRecvError::Empty,
            broadcast::error::TryRecvError::Closed => TryRecvError::Closed,
            broadcast::error::TryRecvError::Lagged(n) => TryRecvError::Lagged(n),
        }
    }
}

impl From<globset::Error> for RecvError {
    fn from(err: globset::Error) -> Self {
        RecvError::InvalidPattern(err.to_string())
    }
}

impl From<TryRecvError> for RecvError {
    fn from(err: TryRecvError) -> Self {
        match err {
            TryRecvError::Empty => RecvError::Timeout,
            TryRecvError::Closed => RecvError::Closed,
            TryRecvError::Lagged(n) => RecvError::Lagged(n),
        }
    }
}

#[cfg(test)]
mod tests {
    use globset::Glob;

    use super::*;

    #[test]
    fn test_recv_error_display() {
        assert_eq!(RecvError::Closed.to_string(), "channel is closed");
        assert_eq!(
            RecvError::Lagged(10).to_string(),
            "receiver lagged behind by 10 messages"
        );
    }

    #[test]
    fn test_try_recv_error_display() {
        assert_eq!(TryRecvError::Empty.to_string(), "no messages available");
    }

    #[test]
    fn test_broadcast_conversion() {
        let err = broadcast::error::RecvError::Closed;
        let converted: RecvError = err.into();
        assert_eq!(converted, RecvError::Closed);

        let err = broadcast::error::TryRecvError::Lagged(42);
        let converted: TryRecvError = err.into();
        assert_eq!(converted, TryRecvError::Lagged(42));
    }

    #[test]
    fn test_globset_conversion() {
        let glob_err = Glob::new("[").unwrap_err();
        let recv_err: RecvError = glob_err.into();
        match recv_err {
            RecvError::InvalidPattern(_) => {} // Ок
            _ => panic!("Expected InvalidPattern"),
        }
    }

    #[test]
    fn test_try_recv_to_recv_conversion() {
        let err = TryRecvError::Empty;
        let recv: RecvError = err.into();
        assert_eq!(recv, RecvError::Timeout);
    }
}
