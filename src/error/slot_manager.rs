use std::sync::PoisonError;

use thiserror::Error;

/// Ошибки SlotManager'а (migration/rebalance рантайм ошибки).
#[derive(Debug, Error)]
pub enum SlotManagerError {
    /// Попытка запустить миграцию для слота, где миграция уже идёт.
    #[error("migration already active for slot {0}")]
    MigrationActive(u16),

    /// Запрошенная миграция не найдена (нет активной миграции для слота).
    #[error("no active migration for slot {0}")]
    NoActiveMigration(u16),

    /// Слот уже в очереди на миграцию.
    #[error("slot {0} already queued for migration")]
    SlotAlreadyQueued(u16),

    /// Некорректный shard id (вызов с несуществующим шардом).
    #[error("invalid shard id: {0}")]
    InvalidShard(usize),

    /// Ошибка ввода/вывода при миграции (например чтение/запись на диск /
    /// сеть).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid slot {0}")]
    InvalidSlot(u16),

    /// Заблокирован (poisoned) lock — конвертируем `PoisonError` сюда.
    #[error("lock poisoned")]
    PoisonedLock,

    /// Прочая ошибка с текстовым сообщением.
    #[error("{0}")]
    Other(String),
}

/// Удобный алиас результата для SlotManager API.
pub type Result<T> = std::result::Result<T, SlotManagerError>;

/// Преобразование из PoisonError<T> в SlotManagerError::PoisonedLock
impl<T> From<PoisonError<T>> for SlotManagerError {
    fn from(_: PoisonError<T>) -> Self {
        SlotManagerError::PoisonedLock
    }
}

impl From<String> for SlotManagerError {
    fn from(s: String) -> Self {
        SlotManagerError::Other(s)
    }
}

impl From<&str> for SlotManagerError {
    fn from(s: &str) -> Self {
        SlotManagerError::Other(s.to_string())
    }
}
