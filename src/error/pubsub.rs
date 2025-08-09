use thiserror::Error;
use tokio::sync::broadcast;

/// Ошибка при получении сообщений (блокирующее `recv()` и т.п.).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecvError {
    #[error("channel is closed")]
    Closed,

    #[error("operation exceeded the specified timeout")]
    Timeout,

    #[error("message serialization/deserialization error: {0}")]
    SerializationError(String),

    #[error("receiver lagged behind by {0} messages")]
    Lagged(u64),

    #[error("invalid glob pattern for subscription: {0}")]
    InvalidPattern(String),

    #[error("channel not found (attempt to operate on a non-existent channel): {0}")]
    ChannelNotFound(String),

    #[error("channel subscriber limit exceeded")]
    SubscriberLimitExceeded,
}

/// Ошибка при неблокирующем получении сообщений (`try_recv()`).
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

/// Удобное конвертирующее преобразование — используется в местах,
/// где `TryRecvError` нужно "поднять" в `RecvError`.
///
/// ВАЖНО: `TryRecvError::Empty` здесь маппится в `RecvError::Timeout`.
/// Это сделано для совместимости с местами где `Err(e).into()` приводило
/// к `RecvError` и ожидалось, что пустая очередь будет считаться
/// "не найдено / нет сообщения". При желании можно убрать это
/// преобразование и делать явную обработку `Empty` там, где это важно.
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
        // invalid pattern -> globset error -> RecvError::InvalidPattern
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
        // Следует понимать: Empty преобразуется в Timeout (см. комментарий выше)
        assert_eq!(recv, RecvError::Timeout);
    }
}
