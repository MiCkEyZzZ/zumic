use thiserror::Error;
use tokio::sync::broadcast;

/// Ошибка при получении сообщений.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecvError {
    #[error("channel is closed")]
    Closed,

    #[error("receiver lagged behind by {0} messages")]
    Lagged(u64),
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
