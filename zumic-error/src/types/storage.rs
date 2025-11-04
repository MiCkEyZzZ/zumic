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

    /// Тест проверяет: статус и клиентское сообщение для варианта KeyNotFound.
    #[test]
    fn test_key_not_found() {
        let err = StorageError::KeyNotFound {
            key: "user:123".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::NotFound);
        assert_eq!(err.client_message(), "Key not found");
    }

    /// Тест проверяет: статус и метки (metrics_tags) для варианта WrongType.
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

    /// Тест проверяет: статус DiskFull и критичность статуса (если
    /// реализовано).
    #[test]
    fn test_disk_full() {
        let err = StorageError::DiskFull {
            available: 1024,
            required: 2048,
        };
        assert_eq!(err.status_code(), StatusCode::DiskFull);
        assert!(err.status_code().is_critical());
    }

    /// Тест проверяет: статус, клиентское сообщение и Display для KeyExists,
    /// InvalidKey и InvalidValue.
    #[test]
    fn test_key_exists_and_invalids() {
        let exists = StorageError::KeyExists { key: "k".into() };
        assert_eq!(exists.status_code(), StatusCode::AlreadyExists);
        assert_eq!(exists.client_message(), "Key already exists");

        let invalid_key = StorageError::InvalidKey {
            key: "bad:key".into(),
            reason: "contains space".into(),
        };
        assert_eq!(invalid_key.status_code(), StatusCode::InvalidKey);
        assert!(invalid_key.to_string().contains("Invalid key"));
        assert_eq!(invalid_key.client_message(), "Invalid key");

        let invalid_value = StorageError::InvalidValue {
            reason: "not json".into(),
        };
        assert_eq!(invalid_value.status_code(), StatusCode::InvalidValue);
        assert_eq!(invalid_value.client_message(), "Invalid value");
    }

    /// Тест проверяет: статус IndexOutOfBounds, клиентское сообщение и вывод
    /// Display.
    #[test]
    fn test_index_out_of_bounds_and_display() {
        let idx = StorageError::IndexOutOfBounds { index: 10, size: 5 };
        assert_eq!(idx.status_code(), StatusCode::IndexOutOfBounds);
        assert_eq!(idx.client_message(), "Index out of bounds");
        assert!(idx.to_string().contains("out of bounds"));
    }

    /// Тест проверяет: StorageUnavailable и DiskFull — статусы, клиентские
    /// сообщения и Display для DiskFull.
    #[test]
    fn test_storage_unavailable_and_diskfull() {
        let su = StorageError::StorageUnavailable {
            reason: "db locked".into(),
        };
        assert_eq!(su.status_code(), StatusCode::StorageUnavailable);
        assert_eq!(su.client_message(), "Storage temporarily unavailable");

        let df = StorageError::DiskFull {
            available: 0,
            required: 1024,
        };
        assert_eq!(df.status_code(), StatusCode::DiskFull);
        assert_eq!(df.client_message(), "Insufficient storage space");
        // assuming StatusCode has an is_critical or similar; keep check simple
        assert!(df.to_string().contains("Disk full"));
    }

    /// Тест проверяет: CorruptedData, SerializationFailed и
    /// DeserializationFailed — статусы, сообщения и Display.
    #[test]
    fn test_corrupted_and_serialization_errors() {
        let corrupt = StorageError::CorruptedData {
            location: "segment-1".into(),
            reason: "checksum mismatch".into(),
        };
        assert_eq!(corrupt.status_code(), StatusCode::CorruptedData);
        assert_eq!(corrupt.client_message(), "Internal server error");
        assert!(corrupt.to_string().contains("Corrupted data"));

        let ser = StorageError::SerializationFailed {
            type_name: "Msg".into(),
            reason: "json error".into(),
        };
        assert_eq!(ser.status_code(), StatusCode::SerializationFailed);
        assert_eq!(ser.client_message(), "Internal server error");

        let deser = StorageError::DeserializationFailed {
            type_name: "Msg".into(),
            reason: "truncated".into(),
        };
        assert_eq!(deser.status_code(), StatusCode::DeserializationFailed);
        assert_eq!(deser.client_message(), "Internal server error");
    }

    /// Тест проверяет: CompressionFailed и DecompressionFailed — статус и
    /// клиентское сообщение.
    #[test]
    fn test_compression_errors() {
        let comp = StorageError::CompressionFailed {
            reason: "zlib error".into(),
        };
        assert_eq!(comp.status_code(), StatusCode::CompressionFailed);
        assert_eq!(comp.client_message(), "Internal server error");

        let decomp = StorageError::DecompressionFailed {
            reason: "truncated stream".into(),
        };
        assert_eq!(decomp.status_code(), StatusCode::CompressionFailed);
        assert_eq!(decomp.client_message(), "Internal server error");
    }

    /// Тест проверяет: WrongShard (статус, клиентское сообщение и метки
    /// expected/actual) и LockError (статус и Display).
    #[test]
    fn test_wrong_shard_and_lock_error_tags() {
        let ws = StorageError::WrongShard {
            key: "user:1".into(),
            expected: 2,
            actual: 3,
        };
        assert_eq!(ws.status_code(), StatusCode::WrongShard);
        assert_eq!(ws.client_message(), "Key belongs to different shard");
        let tags = ws.metrics_tags();
        assert!(tags.iter().any(|(k, v)| k == &"expected_shard" && v == "2"));
        assert!(tags.iter().any(|(k, v)| k == &"actual_shard" && v == "3"));

        let lock = StorageError::LockError {
            resource: "file.db".into(),
            reason: "poisoned".into(),
        };
        assert_eq!(lock.status_code(), StatusCode::LockError);
        assert_eq!(lock.client_message(), "Internal server error");
        assert!(lock.to_string().contains("Lock error"));
    }

    /// Тест проверяет: SizeLimit (статус, клиентское сообщение и метки
    /// data_type) и NotImplemented (статус и сообщение).
    #[test]
    fn test_size_limit_and_not_implemented() {
        let sl = StorageError::SizeLimit {
            limit: 1024,
            actual: 2048,
            data_type: "value".into(),
        };
        assert_eq!(sl.status_code(), StatusCode::SizeLimit);
        assert!(sl.client_message().contains("size limit exceeded"));
        let tags = sl.metrics_tags();
        assert!(tags.iter().any(|(k, v)| k == &"data_type" && v == "value"));

        let ni = StorageError::NotImplemented {
            operation: "ZADD".into(),
        };
        assert_eq!(ni.status_code(), StatusCode::NotImplemented);
        assert!(ni.client_message().contains("Operation not implemented"));
    }

    /// Тест проверяет: as_any downcast для варианта KeyNotFound и корректность
    /// Display для WrongType.
    #[test]
    fn test_as_any_downcast_and_display_all() {
        // pick a variant and ensure as_any allows downcast
        let err = StorageError::KeyNotFound { key: "k".into() };
        let any_ref = err.as_any();
        assert!(any_ref.downcast_ref::<StorageError>().is_some());

        // Also ensure Display is meaningful for some variants
        let s = StorageError::WrongType {
            key: "k".into(),
            expected: "string".into(),
            actual: "list".into(),
        };
        let disp = s.to_string();
        assert!(disp.contains("Wrong type for key"));
    }
}
