use std::any::Any;

use crate::{ErrorExt, StatusCode};

/// Ошибки Storage Engine
#[derive(Debug, Clone)]
pub enum StorageError {
    /// Ключ не найден
    KeyNotFound { key: String },
    /// Ключ уже существует
    KeyExists { key: String },
    /// Невалидный ключ
    InvalidKey { key: String, reason: String },
    /// Невалидное значение
    InvalidValue { reason: String },
    /// Неверный тип данных для операции
    WrongType {
        key: String,
        expected: String,
        actual: String,
    },
    /// Индекс за пределами диапазона
    IndexOutOfBounds { index: i64, size: usize },
    /// Хранилище недоступно
    StorageUnavailable { reason: String },
    /// Диск заполнен
    DiskFull { available: u64, required: u64 },
    /// Повреждённые данные
    CorruptedData { location: String, reason: String },
    /// Ошибка сериализации
    SerializationFailed { type_name: String, reason: String },
    /// Ошибка десериализации
    DeserializationFailed { type_name: String, reason: String },
    /// Ошибка компрессии
    CompressionFailed { reason: String },
    /// Ошибка декомпрессии
    DecompressionFailed { reason: String },
    /// Неверный шард для ключа
    WrongShard {
        key: String,
        expected: usize,
        actual: usize,
    },
    /// Ошибка блокировки
    LockError { resource: String, reason: String },
    /// Превышен лимит размера
    SizeLimit {
        limit: usize,
        actual: usize,
        data_type: String,
    },
    /// Операция не реализована
    NotImplemented { operation: String },
}

impl std::fmt::Display for StorageError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::KeyNotFound { key } => write!(f, "Key not found: {key}"),
            Self::KeyExists { key } => write!(f, "Key already exists: {key}"),
            Self::InvalidKey { key, reason } => write!(f, "Invalid key '{key}': {reason}"),
            Self::InvalidValue { reason } => write!(f, "Invalid value: {reason}"),
            Self::WrongType {
                key,
                expected,
                actual,
            } => write!(
                f,
                "Wrong type for key '{key}': expected {expected}, got {actual}"
            ),
            Self::IndexOutOfBounds { index, size } => {
                write!(f, "Index {index} out of bounds for size {size}")
            }
            Self::StorageUnavailable { reason } => write!(f, "Storage unavailable: {reason}"),
            Self::DiskFull {
                available,
                required,
            } => {
                write!(
                    f,
                    "Disk full: available {available} bytes, required {required} bytes"
                )
            }
            Self::CorruptedData { location, reason } => {
                write!(f, "Corrupted data at {location}: {reason}")
            }
            Self::SerializationFailed { type_name, reason } => {
                write!(f, "Serialization failed for {type_name}: {reason}")
            }
            Self::DeserializationFailed { type_name, reason } => {
                write!(f, "Deserialization failed for {type_name}: {reason}")
            }
            Self::CompressionFailed { reason } => write!(f, "Compression failed: {reason}"),
            Self::DecompressionFailed { reason } => write!(f, "Decompression failed: {reason}"),
            Self::WrongShard {
                key,
                expected,
                actual,
            } => write!(
                f,
                "Key '{key}' belongs to shard {expected}, but accessed on shard {actual}"
            ),
            Self::LockError { resource, reason } => {
                write!(f, "Lock error on {resource}: {reason}")
            }
            Self::SizeLimit {
                limit,
                actual,
                data_type,
            } => write!(f, "Size limit exceeded for {data_type}: {actual} > {limit}"),
            Self::NotImplemented { operation } => {
                write!(f, "Operation not implemented: {operation}")
            }
        }
    }
}

impl std::error::Error for StorageError {}

impl ErrorExt for StorageError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::KeyNotFound { .. } => StatusCode::NotFound,
            Self::KeyExists { .. } => StatusCode::AlreadyExists,
            Self::InvalidKey { .. } => StatusCode::InvalidKey,
            Self::InvalidValue { .. } => StatusCode::InvalidValue,
            Self::WrongType { .. } => StatusCode::WrongType,
            Self::IndexOutOfBounds { .. } => StatusCode::IndexOutOfBounds,
            Self::StorageUnavailable { .. } => StatusCode::StorageUnavailable,
            Self::DiskFull { .. } => StatusCode::DiskFull,
            Self::CorruptedData { .. } => StatusCode::CorruptedData,
            Self::SerializationFailed { .. } => StatusCode::SerializationFailed,
            Self::DeserializationFailed { .. } => StatusCode::DeserializationFailed,
            Self::CompressionFailed { .. } | Self::DecompressionFailed { .. } => {
                StatusCode::CompressionFailed
            }
            Self::WrongShard { .. } => StatusCode::WrongShard,
            Self::LockError { .. } => StatusCode::LockError,
            Self::SizeLimit { .. } => StatusCode::SizeLimit,
            Self::NotImplemented { .. } => StatusCode::NotImplemented,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::KeyNotFound { .. } => "Key not found".to_string(),
            Self::KeyExists { .. } => "Key already exists".to_string(),
            Self::InvalidKey { .. } => "Invalid key".to_string(),
            Self::InvalidValue { .. } => "Invalid value".to_string(),
            Self::WrongType { expected, .. } => format!("Wrong type: expected {expected}"),
            Self::IndexOutOfBounds { .. } => "Index out of bounds".to_string(),
            Self::StorageUnavailable { .. } => "Storage temporarily unavailable".to_string(),
            Self::DiskFull { .. } => "Insufficient storage space".to_string(),
            Self::CorruptedData { .. }
            | Self::SerializationFailed { .. }
            | Self::DeserializationFailed { .. }
            | Self::CompressionFailed { .. }
            | Self::DecompressionFailed { .. }
            | Self::LockError { .. } => "Internal server error".to_string(),
            Self::WrongShard { .. } => "Key belongs to different shard".to_string(),
            Self::SizeLimit { data_type, .. } => format!("{data_type} size limit exceeded"),
            Self::NotImplemented { operation } => format!("Operation not implemented: {operation}"),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "storage".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::WrongType {
                expected, actual, ..
            } => {
                tags.push(("expected_type", expected.clone()));
                tags.push(("actual_type", actual.clone()));
            }
            Self::WrongShard {
                expected, actual, ..
            } => {
                tags.push(("expected_shard", expected.to_string()));
                tags.push(("actual_shard", actual.to_string()));
            }
            Self::SizeLimit { data_type, .. } => {
                tags.push(("data_type", data_type.clone()));
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
    fn test_key_not_found() {
        let err = StorageError::KeyNotFound {
            key: "user:123".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::NotFound);
        assert_eq!(err.client_message(), "Key not found");
    }

    #[test]
    fn test_wrong_type() {
        let err = StorageError::WrongType {
            key: "counter".to_string(),
            expected: "string".to_string(),
            actual: "hash".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::WrongType);

        let tags = err.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| k == &"expected_type" && v == "string"));
        assert!(tags.iter().any(|(k, v)| k == &"actual_type" && v == "hash"));
    }

    #[test]
    fn test_disk_full() {
        let err = StorageError::DiskFull {
            available: 1024,
            required: 2048,
        };
        assert_eq!(err.status_code(), StatusCode::DiskFull);
        assert!(err.status_code().is_critical());
    }
}
