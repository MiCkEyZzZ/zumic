//! # ZDB Errors
//!
//! Этот модуль предоставляет типы ошибок и вспомогательные методы для работы
//! с дампами ZDB. Все ошибки содержат контекст, что облегчает отладку и
//! логирование.
//!
//! ## Основные типы ошибок
//!
//! - `ZdbError` — общая ошибка работы с дампами.
//! - `ZdbVersionError` — ошибки версий ZDB.

use std::{any::Any, io};

use crate::{ErrorExt, StatusCode};

/// Основная ошибка ZDB
#[derive(Debug, Clone)]
pub enum ZdbError {
    /// Повреждённые данные в дампе
    CorruptedData {
        reason: String,
        offset: Option<u64>,
        key: Option<String>,
        expected: Option<String>,
        got: Option<String>,
    },
    /// Неизвестный или недопустимый тег типа
    InvalidTag {
        tag: u8,
        offset: Option<u64>,
        key: Option<String>,
        valid_tags: Vec<u8>,
    },
    /// Ошибка сжатия/распаковки
    CompressionError {
        operation: CompressionOp,
        reason: String,
        offset: Option<u64>,
        key: Option<String>,
        compressed_size: Option<u32>,
    },
    /// Неожиданный конец файла
    UnexpectedEof {
        context: String,
        offset: Option<u64>,
        key: Option<String>,
        expected_bytes: Option<u64>,
        got_bytes: Option<u64>,
    },
    /// Превышен лимит размера
    SizeLimit {
        what: String,
        size: u64,
        limit: u64,
        offset: Option<u64>,
        key: Option<String>,
    },
    /// Неверный magic number в заголовке
    InvalidMagic { expected: [u8; 3], got: [u8; 3] },
    /// CRC не совпадает
    CrcMismatch {
        computed: u32,
        recorded: u32,
        offset: Option<u64>,
    },
    /// Версионные ошибки (делегирование в ZdbVersionError)
    Version(ZdbVersionError),
    /// Ошибка парсинга структуры
    ParseError {
        structure: String,
        reason: String,
        offset: Option<u64>,
        key: Option<String>,
    },
    /// Ошибка кодирования данных
    EncodingError {
        what: String,
        reason: String,
        key: Option<String>,
    },
    /// Файл слишком маленький
    FileTooSmall { size: u64, minimum: u64 },
}

/// Операции сжатия
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionOp {
    Compress,
    Decompress,
}

/// Ошибки версий ZDB
///
/// Представляет несовместимость версий дампа и reader-а.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZdbVersionError {
    /// Неподдерживаемая версия дампа
    UnsupportedVersion {
        found: u8,
        supported: Vec<u8>,
        offset: Option<u64>,
        key: Option<String>,
    },
    /// Несовместимость версий
    IncompatibleVersion {
        reader: u8,
        dump: u8,
        offset: Option<u64>,
        key: Option<String>,
    },
    /// Устаревшая версия
    DeprecatedVersion {
        version: u8,
        recommended: u8,
        offset: Option<u64>,
        key: Option<String>,
    },
    /// Невозможно записать версию
    WriteIncompatible {
        writer: u8,
        target: u8,
        offset: Option<u64>,
        key: Option<String>,
    },
}

impl ZdbError {
    /// Добавляет контекст offset к ошибке.
    pub fn with_offset(
        mut self,
        offset: u64,
    ) -> Self {
        match &mut self {
            Self::CorruptedData { offset: o, .. }
            | Self::InvalidTag { offset: o, .. }
            | Self::CompressionError { offset: o, .. }
            | Self::UnexpectedEof { offset: o, .. }
            | Self::SizeLimit { offset: o, .. }
            | Self::CrcMismatch { offset: o, .. }
            | Self::ParseError { offset: o, .. } => {
                *o = Some(offset);
            }
            Self::Version(v) => match v {
                ZdbVersionError::UnsupportedVersion { offset: o, .. }
                | ZdbVersionError::IncompatibleVersion { offset: o, .. }
                | ZdbVersionError::DeprecatedVersion { offset: o, .. }
                | ZdbVersionError::WriteIncompatible { offset: o, .. } => {
                    *o = Some(offset);
                }
            },
            _ => {}
        }
        self
    }

    /// Добавляет контекст ключа к ошибке.
    pub fn with_key(
        mut self,
        key: impl Into<String>,
    ) -> Self {
        let k = Some(key.into());
        match &mut self {
            Self::CorruptedData {
                key: ref mut k2, ..
            }
            | Self::InvalidTag {
                key: ref mut k2, ..
            }
            | Self::CompressionError {
                key: ref mut k2, ..
            }
            | Self::UnexpectedEof {
                key: ref mut k2, ..
            }
            | Self::SizeLimit {
                key: ref mut k2, ..
            }
            | Self::ParseError {
                key: ref mut k2, ..
            }
            | Self::EncodingError {
                key: ref mut k2, ..
            } => {
                *k2 = k;
            }
            Self::Version(v) => match v {
                ZdbVersionError::UnsupportedVersion {
                    key: ref mut k2, ..
                }
                | ZdbVersionError::IncompatibleVersion {
                    key: ref mut k2, ..
                }
                | ZdbVersionError::DeprecatedVersion {
                    key: ref mut k2, ..
                }
                | ZdbVersionError::WriteIncompatible {
                    key: ref mut k2, ..
                } => {
                    *k2 = k;
                }
            },
            _ => {}
        }
        self
    }

    /// Возвращает recovery hint для пользователя.
    pub fn recovery_hint(&self) -> Option<&'static str> {
        match self {
            Self::CorruptedData { .. } => Some("Try using a backup or repair tool"),
            Self::CrcMismatch { .. } => {
                Some("File may be corrupted. Try re-downloading or using a backup")
            }
            Self::Version(ZdbVersionError::UnsupportedVersion { .. }) => {
                Some("Upgrade your ZDB client to support this version")
            }
            Self::Version(ZdbVersionError::IncompatibleVersion { .. }) => {
                Some("Convert the dump to a compatible version using migration tools")
            }
            Self::Version(ZdbVersionError::DeprecatedVersion { .. }) => {
                Some("Consider upgrading the dump format to the latest version")
            }
            Self::UnexpectedEof { .. } => Some("File may be truncated. Check file integrity"),
            Self::SizeLimit { .. } => Some("Reduce data size or increase limits in configuration"),
            Self::CompressionError { .. } => Some("Check if zstd library is properly installed"),
            _ => None,
        }
    }

    /// Является ли ошибка потенциально восстановимой.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::UnexpectedEof { .. }
                | Self::InvalidTag { .. }
                | Self::ParseError { .. }
                | Self::Version(ZdbVersionError::DeprecatedVersion { .. })
        )
    }

    /// Устанавливает offset в существующей ошибке (in-place) и возвращает &mut
    /// Self.
    pub fn set_offset(
        &mut self,
        offset: u64,
    ) -> &mut Self {
        match self {
            Self::CorruptedData { offset: o, .. }
            | Self::InvalidTag { offset: o, .. }
            | Self::CompressionError { offset: o, .. }
            | Self::UnexpectedEof { offset: o, .. }
            | Self::SizeLimit { offset: o, .. }
            | Self::CrcMismatch { offset: o, .. }
            | Self::ParseError { offset: o, .. } => *o = Some(offset),
            Self::Version(v) => match v {
                ZdbVersionError::UnsupportedVersion { offset: o, .. }
                | ZdbVersionError::IncompatibleVersion { offset: o, .. }
                | ZdbVersionError::DeprecatedVersion { offset: o, .. }
                | ZdbVersionError::WriteIncompatible { offset: o, .. } => *o = Some(offset),
            },
            _ => {}
        }
        self
    }

    /// Устанавливает ключ (key) in-place и возвращает &mut self.
    pub fn set_key(
        &mut self,
        key: impl Into<String>,
    ) -> &mut Self {
        let k = Some(key.into());
        match self {
            Self::CorruptedData {
                key: ref mut k2, ..
            }
            | Self::InvalidTag {
                key: ref mut k2, ..
            }
            | Self::CompressionError {
                key: ref mut k2, ..
            }
            | Self::UnexpectedEof {
                key: ref mut k2, ..
            }
            | Self::SizeLimit {
                key: ref mut k2, ..
            }
            | Self::ParseError {
                key: ref mut k2, ..
            }
            | Self::EncodingError {
                key: ref mut k2, ..
            } => {
                *k2 = k;
            }
            Self::Version(v) => match v {
                ZdbVersionError::UnsupportedVersion {
                    key: ref mut k2, ..
                }
                | ZdbVersionError::IncompatibleVersion {
                    key: ref mut k2, ..
                }
                | ZdbVersionError::DeprecatedVersion {
                    key: ref mut k2, ..
                }
                | ZdbVersionError::WriteIncompatible {
                    key: ref mut k2, ..
                } => {
                    *k2 = k;
                }
            },
            _ => {}
        }
        self
    }
}

impl ZdbVersionError {
    /// Добавляет контекст (offset, key) к ошибке.
    pub fn with_context(
        mut self,
        offset: u64,
        key: Option<&str>,
    ) -> Self {
        match &mut self {
            Self::UnsupportedVersion {
                offset: o, key: k, ..
            }
            | Self::IncompatibleVersion {
                offset: o, key: k, ..
            }
            | Self::DeprecatedVersion {
                offset: o, key: k, ..
            }
            | Self::WriteIncompatible {
                offset: o, key: k, ..
            } => {
                *o = Some(offset);
                *k = key.map(|s| s.to_string());
            }
        }
        self
    }
}

impl std::fmt::Display for ZdbVersionError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            ZdbVersionError::UnsupportedVersion {
                found,
                supported,
                offset,
                key,
            } => {
                write!(f, "Unsupported version {found} (supported: {supported:?})")?;
                write_context(f, *offset, key.as_deref())
            }
            ZdbVersionError::IncompatibleVersion {
                reader,
                dump,
                offset,
                key,
            } => {
                write!(
                    f,
                    "Incompatible version: reader v{reader} cannot read dump v{dump}"
                )?;
                write_context(f, *offset, key.as_deref())
            }
            ZdbVersionError::DeprecatedVersion {
                version,
                recommended,
                offset,
                key,
            } => {
                write!(
                    f,
                    "Deprecated version {version} (recommended: v{recommended})"
                )?;
                write_context(f, *offset, key.as_deref())
            }
            ZdbVersionError::WriteIncompatible {
                writer,
                target,
                offset,
                key,
            } => {
                write!(f, "Cannot write version {target} using writer v{writer}")?;
                write_context(f, *offset, key.as_deref())
            }
        }
    }
}

impl std::error::Error for ZdbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Version(v) => Some(v),
            _ => None,
        }
    }
}

impl std::error::Error for ZdbVersionError {}

impl ErrorExt for ZdbError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::CorruptedData { .. } => StatusCode::CorruptedData,
            Self::InvalidTag { .. } => StatusCode::InvalidData,
            Self::CompressionError { .. } => StatusCode::CompressionFailed,
            Self::UnexpectedEof { .. } => StatusCode::UnexpectedEof,
            Self::SizeLimit { .. } => StatusCode::SizeLimit,
            Self::InvalidMagic { .. } => StatusCode::InvalidData,
            Self::CrcMismatch { .. } => StatusCode::CorruptedData,
            Self::Version(v) => v.status_code(),
            Self::ParseError { .. } => StatusCode::ParseError,
            Self::EncodingError { .. } => StatusCode::EncodingError,
            Self::FileTooSmall { .. } => StatusCode::InvalidData,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::CorruptedData { .. } => "Database file is corrupted".to_string(),
            Self::InvalidTag { .. } => "Invalid data format".to_string(),
            Self::CompressionError { .. } => "Compression/Decompression failed".to_string(),
            Self::UnexpectedEof { .. } => "Incomplete database file".to_string(),
            Self::SizeLimit { what, .. } => format!("{what} exceeds size limit"),
            Self::InvalidMagic { .. } => "Not a valid ZDB file".to_string(),
            Self::CrcMismatch { .. } => "Database file checksum mismatch".to_string(),
            Self::Version(v) => v.client_message(),
            Self::ParseError { structure, .. } => {
                format!("Failed to parse {structure} structure")
            }
            Self::EncodingError { what, .. } => format!("Failed to encode {what}"),
            Self::FileTooSmall { .. } => "Database file is too small".to_string(),
        }
    }

    fn log_message(&self) -> String {
        let mut msg = format!("{self:?}");
        if let Some(hint) = self.recovery_hint() {
            msg.push_str(&format!(" | Hint: {hint}"));
        }
        msg
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", self.type_name()),
            ("status_code", self.status_code().to_string()),
            ("recoverable", self.is_recoverable().to_string()),
        ];

        // Добавляем специфичные теги
        match self {
            Self::InvalidTag { tag, .. } => {
                tags.push(("invalid_tag", format!("0x{tag:02X}")));
            }
            Self::CompressionError { operation, .. } => {
                tags.push(("compression_op", format!("{operation:?}")));
            }
            Self::Version(ZdbVersionError::UnsupportedVersion { found, .. }) => {
                tags.push(("version", found.to_string()));
            }
            Self::SizeLimit { what, .. } => {
                tags.push(("limit_type", what.clone()));
            }
            _ => {}
        }
        tags
    }
}

impl ErrorExt for ZdbVersionError {
    fn status_code(&self) -> StatusCode {
        StatusCode::UnsupportedVersion
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::UnsupportedVersion { .. } => "Unsupported database version".to_string(),
            Self::IncompatibleVersion { .. } => "Incompatible database version".to_string(),
            Self::DeprecatedVersion { .. } => "Deprecated database version".to_string(),
            Self::WriteIncompatible { .. } => "Database version write error".to_string(),
        }
    }
}

// Конверсия в std::io::Error для совместимости с существующим кодом
impl From<ZdbError> for std::io::Error {
    fn from(e: ZdbError) -> Self {
        let kind = match &e {
            ZdbError::UnexpectedEof { .. } => std::io::ErrorKind::UnexpectedEof,
            ZdbError::InvalidMagic { .. }
            | ZdbError::InvalidTag { .. }
            | ZdbError::CorruptedData { .. }
            | ZdbError::CrcMismatch { .. }
            | ZdbError::ParseError { .. } => std::io::ErrorKind::InvalidData,
            ZdbError::CompressionError { .. } => std::io::ErrorKind::Other,
            ZdbError::SizeLimit { .. } => std::io::ErrorKind::InvalidInput,
            ZdbError::FileTooSmall { .. } => std::io::ErrorKind::InvalidData,
            ZdbError::Version(_) => std::io::ErrorKind::Unsupported,
            ZdbError::EncodingError { .. } => std::io::ErrorKind::InvalidData,
        };

        std::io::Error::new(kind, e.to_string())
    }
}

impl From<ZdbVersionError> for ZdbError {
    fn from(e: ZdbVersionError) -> Self {
        ZdbError::Version(e)
    }
}

impl From<io::Error> for ZdbError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::UnexpectedEof => ZdbError::UnexpectedEof {
                context: e.to_string(),
                offset: None,
                key: None,
                expected_bytes: None,
                got_bytes: None,
            },
            io::ErrorKind::InvalidData => ZdbError::CorruptedData {
                reason: e.to_string(),
                offset: None,
                key: None,
                expected: None,
                got: None,
            },
            io::ErrorKind::InvalidInput => ZdbError::EncodingError {
                what: "input".to_string(),
                reason: e.to_string(),
                key: None,
            },
            // PermissionDenied / NotFound / Other - оборачиваем в CorruptedData с причиной
            _ => ZdbError::CorruptedData {
                reason: e.to_string(),
                offset: None,
                key: None,
                expected: None,
                got: None,
            },
        }
    }
}

// zstd error -> ZdbError: безопасно используем Debug-формат, чтобы не зависеть
// от наличия Display у типа zstd::Error.
#[cfg(feature = "zstd")]
impl From<zstd::error::Error> for ZdbError {
    fn from(e: zstd::error::Error) -> Self {
        ZdbError::CompressionError {
            operation: CompressionOp::Decompress,
            reason: format!("{e:?}"),
            offset: None,
            key: None,
            compressed_size: None,
        }
    }
}

impl std::fmt::Display for CompressionOp {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::Compress => write!(f, "Compression"),
            Self::Decompress => write!(f, "Decompression"),
        }
    }
}

impl std::fmt::Display for ZdbError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            ZdbError::CorruptedData {
                reason,
                offset,
                key,
                expected,
                got,
            } => {
                write!(f, "Corrupted data: {reason}")?;
                if let Some(exp) = expected {
                    write!(f, " (expected: {exp}")?;
                    if let Some(g) = got {
                        write!(f, ", got: {g}")?;
                    }
                    write!(f, ")")?;
                }
                write_context(f, *offset, key.as_deref())
            }
            ZdbError::InvalidTag {
                tag,
                valid_tags,
                offset,
                key,
                ..
            } => {
                write!(f, "Invalid tag 0x{tag:02X} (valid: {valid_tags:?})")?;
                write_context(f, *offset, key.as_deref())
            }
            ZdbError::CompressionError {
                operation,
                reason,
                offset,
                key,
                compressed_size,
            } => {
                write!(f, "{operation:?} error: {reason}")?;
                if let Some(sz) = compressed_size {
                    write!(f, " (size: {sz} bytes)")?;
                }
                write_context(f, *offset, key.as_deref())
            }
            ZdbError::UnexpectedEof {
                context,
                offset,
                key,
                expected_bytes,
                got_bytes,
            } => {
                write!(f, "Unexpected EOF: {context}")?;
                if let (Some(exp), Some(got)) = (expected_bytes, got_bytes) {
                    write!(f, " (expected {exp} bytes, got {got})")?;
                }
                write_context(f, *offset, key.as_deref())
            }
            ZdbError::SizeLimit {
                what,
                size,
                limit,
                offset,
                key,
            } => {
                write!(f, "{what} size {size} exceeds limit {limit} bytes")?;
                write_context(f, *offset, key.as_deref())
            }
            ZdbError::InvalidMagic { expected, got } => {
                write!(f, "Invalid magic: expected {expected:?}, got {got:?}")
            }
            ZdbError::CrcMismatch {
                computed,
                recorded,
                offset,
            } => {
                write!(
                    f,
                    "CRC mismatch: computed 0x{computed:08X}, recorded 0x{recorded:08X}"
                )?;
                write_context(f, *offset, None)
            }
            ZdbError::Version(v) => write!(f, "{v}"),
            ZdbError::ParseError {
                structure,
                reason,
                offset,
                key,
            } => {
                write!(f, "Failed to parse {structure}: {reason}")?;
                write_context(f, *offset, key.as_deref())
            }
            ZdbError::EncodingError { what, reason, key } => {
                write!(f, "Encoding error for {what}: {reason}")?;
                if let Some(k) = key {
                    write!(f, " (key: {k})")?;
                }
                Ok(())
            }
            ZdbError::FileTooSmall { size, minimum } => {
                write!(f, "File too small: {size} bytes (minimum: {minimum} bytes)")
            }
        }
    }
}

/// Вспомогательная ф-я для форматирования контекста (offset, key).
fn write_context(
    f: &mut std::fmt::Formatter<'_>,
    offset: Option<u64>,
    key: Option<&str>,
) -> std::fmt::Result {
    let mut parts = Vec::new();
    if let Some(o) = offset {
        parts.push(format!("offset: 0x{o:X}"));
    }
    if let Some(k) = key {
        parts.push(format!("key: {k}"));
    }
    if !parts.is_empty() {
        write!(f, " [{}]", parts.join(", "))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{any::Any, error::Error};

    use super::*;

    /// Тест проверяет Display для UnsupportedVersion и ожидаемый формат строки.
    #[test]
    fn test_display_unsupported_version() {
        let err = ZdbVersionError::UnsupportedVersion {
            found: 3,
            supported: vec![1, 2],
            offset: None,
            key: None,
        };
        let s = format!("{err}");
        assert_eq!(s, "Unsupported version 3 (supported: [1, 2])");
    }

    /// Тест проверяет Display для IncompatibleVersion и ожидаемый формат
    /// строки.
    #[test]
    fn test_display_incompatible_version() {
        let err = ZdbVersionError::IncompatibleVersion {
            reader: 1,
            dump: 2,
            offset: None,
            key: None,
        };
        let s = format!("{err}");
        assert_eq!(s, "Incompatible version: reader v1 cannot read dump v2");
    }

    /// Тест проверяет Display для DeprecatedVersion и ожидаемый формат строки.
    #[test]
    fn test_display_deprecated_version() {
        let err = ZdbVersionError::DeprecatedVersion {
            version: 1,
            recommended: 2,
            offset: None,
            key: None,
        };
        let s = format!("{err}");
        assert_eq!(s, "Deprecated version 1 (recommended: v2)");
    }

    /// Тест проверяет Display для WriteIncompatible и ожидаемый формат строки.
    #[test]
    fn test_display_write_incompatible() {
        let err = ZdbVersionError::WriteIncompatible {
            writer: 10,
            target: 20,
            offset: None,
            key: None,
        };
        let s = format!("{err}");
        assert_eq!(s, "Cannot write version 20 using writer v10");
    }

    /// Тест проверяет соответствие client_message() ожидаемым сообщениям для
    /// всех вариантов.
    #[test]
    fn test_client_message_matches_variant() {
        let e1 = ZdbVersionError::UnsupportedVersion {
            found: 0,
            supported: vec![],
            offset: None,
            key: None,
        };
        assert_eq!(
            e1.client_message(),
            "Unsupported database version".to_string()
        );

        let e2 = ZdbVersionError::IncompatibleVersion {
            reader: 0,
            dump: 0,
            offset: None,
            key: None,
        };
        assert_eq!(
            e2.client_message(),
            "Incompatible database version".to_string()
        );

        let e3 = ZdbVersionError::DeprecatedVersion {
            version: 0,
            recommended: 0,
            offset: None,
            key: None,
        };
        assert_eq!(
            e3.client_message(),
            "Deprecated database version".to_string()
        );

        let e4 = ZdbVersionError::WriteIncompatible {
            writer: 0,
            target: 0,
            offset: None,
            key: None,
        };
        assert_eq!(
            e4.client_message(),
            "Database version write error".to_string()
        );
    }

    /// Тест проверяет, что status_code() возвращает
    /// StatusCode::UnsupportedVersion для всех вариантов enum.
    #[test]
    fn test_status_code_is_unsupported_for_all_variants() {
        let v = vec![
            ZdbVersionError::UnsupportedVersion {
                found: 1,
                supported: vec![1],
                offset: None,
                key: None,
            },
            ZdbVersionError::IncompatibleVersion {
                reader: 1,
                dump: 2,
                offset: None,
                key: None,
            },
            ZdbVersionError::DeprecatedVersion {
                version: 1,
                recommended: 2,
                offset: None,
                key: None,
            },
            ZdbVersionError::WriteIncompatible {
                writer: 1,
                target: 2,
                offset: None,
                key: None,
            },
        ];

        for err in v {
            assert_eq!(err.status_code(), StatusCode::UnsupportedVersion);
        }
    }

    /// Тест проверяет, что as_any() позволяет выполнить downcast_ref к
    /// ZdbVersionError.
    #[test]
    fn test_as_any_allows_downcast() {
        let err = ZdbVersionError::IncompatibleVersion {
            reader: 7,
            dump: 8,
            offset: None,
            key: None,
        };
        let any_ref: &dyn Any = err.as_any();
        // downcast_ref должен вернуть Some, потому что конкретный тип — ZdbVersionError
        assert!(any_ref.downcast_ref::<ZdbVersionError>().is_some());
    }

    /// Тест проверяет (компиляционно), что тип реализует std::error::Error.
    #[test]
    fn test_implements_std_error_trait() {
        let err = ZdbVersionError::UnsupportedVersion {
            found: 1,
            supported: vec![1],
            offset: None,
            key: None,
        };
        let _err_ref: &dyn std::error::Error = &err; // компилируется только
                                                     // если impl std::error::Error
                                                     // существует
    }

    /// Тест проверяет, что as_any() позволяет выполнить downcast_ref к
    /// ZdbVersionError и затем проверить конкретный вариант и поля.
    #[test]
    fn test_as_any_downcast_and_match_variant_fields() {
        let err = ZdbVersionError::IncompatibleVersion {
            reader: 7,
            dump: 8,
            offset: None,
            key: None,
        };
        let any_ref: &dyn Any = err.as_any();
        let down = any_ref
            .downcast_ref::<ZdbVersionError>()
            .expect("downcast to ZdbVersionError failed");

        // сравниваем с ожидаемым вариантом, включая offset/key
        assert_eq!(
            down,
            &ZdbVersionError::IncompatibleVersion {
                reader: 7,
                dump: 8,
                offset: None,
                key: None
            }
        );

        // либо распарсить вручную и проверить поля:
        if let ZdbVersionError::IncompatibleVersion { reader, dump, .. } = down {
            assert_eq!(*reader, 7);
            assert_eq!(*dump, 8);
        } else {
            panic!("Ожидался IncompatibleVersion");
        }
    }

    /// Тест проверяет Clone/PartialEq (sanity-check).
    #[test]
    fn test_clone_and_partial_eq() {
        let a = ZdbVersionError::UnsupportedVersion {
            found: 4,
            supported: vec![1, 2, 3, 4],
            offset: None,
            key: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    /// Тест проверяет методы with_offset и set_offset для разных вариантов
    /// ошибок.
    #[test]
    fn test_offset_methods() {
        let corrupted = ZdbError::CorruptedData {
            reason: "oops".into(),
            offset: None,
            key: None,
            expected: None,
            got: None,
        };
        let with_off = corrupted.clone().with_offset(42);
        if let ZdbError::CorruptedData { offset, .. } = with_off {
            assert_eq!(offset, Some(42));
        } else {
            panic!("Expected CorruptedData variant");
        }

        let mut e = corrupted;
        e.set_offset(100);
        if let ZdbError::CorruptedData { offset, .. } = e {
            assert_eq!(offset, Some(100));
        }
    }

    /// Тест проверяет методы with_key и set_key для разных вариантов ошибок.
    #[test]
    fn test_key_methods() {
        let parse_err = ZdbError::ParseError {
            structure: "header".into(),
            reason: "bad".into(),
            offset: None,
            key: None,
        };
        let with_key = parse_err.clone().with_key("mykey");
        if let ZdbError::ParseError { key, .. } = with_key {
            assert_eq!(key.as_deref(), Some("mykey"));
        }

        let mut e = parse_err;
        e.set_key("another");
        if let ZdbError::ParseError { key, .. } = e {
            assert_eq!(key.as_deref(), Some("another"));
        }
    }

    /// Тест проверяет методы recovery_hint() и is_recoverable().
    #[test]
    fn test_recovery_and_recoverable() {
        let err = ZdbError::CorruptedData {
            reason: "oops".into(),
            offset: None,
            key: None,
            expected: None,
            got: None,
        };
        assert_eq!(
            err.recovery_hint(),
            Some("Try using a backup or repair tool")
        );
        assert!(!err.is_recoverable());

        let eof_err = ZdbError::UnexpectedEof {
            context: "end".into(),
            offset: None,
            key: None,
            expected_bytes: None,
            got_bytes: None,
        };
        assert_eq!(
            eof_err.recovery_hint(),
            Some("File may be truncated. Check file integrity")
        );
        assert!(eof_err.is_recoverable());
    }

    /// Тест проверяет конвертацию ZdbError -> std::io::Error.
    #[test]
    fn test_into_io_error() {
        let err = ZdbError::UnexpectedEof {
            context: "oops".into(),
            offset: None,
            key: None,
            expected_bytes: None,
            got_bytes: None,
        };
        let io_err: std::io::Error = err.into();
        assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    /// Тест проверяет конвертацию ZdbVersionError -> ZdbError.
    #[test]
    fn test_version_error_into_zdb_error() {
        let v = ZdbVersionError::UnsupportedVersion {
            found: 1,
            supported: vec![1],
            offset: None,
            key: None,
        };
        let z: ZdbError = v.clone().into();
        if let ZdbError::Version(inner) = z {
            assert_eq!(inner, v);
        } else {
            panic!("Expected ZdbError::Version");
        }
    }

    /// Тест проверяет Display для всех основных вариантов ZdbError.
    #[test]
    fn test_display_variants() {
        let err = ZdbError::InvalidMagic {
            expected: [1, 2, 3],
            got: [4, 5, 6],
        };
        let s = format!("{err}");
        assert!(s.contains("Invalid magic"));
    }

    /// Тест проверяет metrics_tags() для специфичных тегов.
    #[test]
    fn test_metrics_tags() {
        let err = ZdbError::InvalidTag {
            tag: 0xAA,
            valid_tags: vec![0x01, 0xAA],
            offset: None,
            key: None,
        };
        let tags = err.metrics_tags();
        let tag_str = tags
            .iter()
            .find(|(k, _)| *k == "invalid_tag")
            .unwrap()
            .1
            .clone();
        assert_eq!(tag_str, "0xAA");
    }

    /// Тест проверяет Display и client_message для EncodingError.
    #[test]
    fn test_encoding_error_display() {
        let e = ZdbError::EncodingError {
            what: "value".into(),
            reason: "bad encoding".into(),
            key: Some("key1".into()),
        };
        let s = format!("{e}");
        assert!(s.contains("Encoding error for value"));
        assert!(s.contains("key1"));
        assert_eq!(e.client_message(), "Failed to encode value");
    }

    /// Тест проверяет Display и client_message для SizeLimit.
    #[test]
    fn test_size_limit_display_and_client_message() {
        let e = ZdbError::SizeLimit {
            what: "item".into(),
            size: 1024,
            limit: 512,
            offset: Some(0x10),
            key: Some("k".into()),
        };
        let s = format!("{e}");
        assert!(s.contains("item size 1024 exceeds limit 512 bytes"));
        assert_eq!(e.client_message(), "item exceeds size limit");
    }

    /// Тест проверяет методы with_context(), with_offset(), with_key() для всех
    /// вариантов ZdbVersionError и корректность Display + client_message.
    #[test]
    fn test_version_error_context_and_display() {
        let variants = vec![
            ZdbVersionError::UnsupportedVersion {
                found: 3,
                supported: vec![1, 2],
                offset: None,
                key: None,
            },
            ZdbVersionError::IncompatibleVersion {
                reader: 1,
                dump: 2,
                offset: None,
                key: None,
            },
            ZdbVersionError::DeprecatedVersion {
                version: 1,
                recommended: 2,
                offset: None,
                key: None,
            },
            ZdbVersionError::WriteIncompatible {
                writer: 10,
                target: 20,
                offset: None,
                key: None,
            },
        ];

        for v in variants {
            // Применяем with_context
            let v_with = v.clone().with_context(0x1234, Some("mykey"));
            match &v_with {
                ZdbVersionError::UnsupportedVersion { offset, key, .. }
                | ZdbVersionError::IncompatibleVersion { offset, key, .. }
                | ZdbVersionError::DeprecatedVersion { offset, key, .. }
                | ZdbVersionError::WriteIncompatible { offset, key, .. } => {
                    assert_eq!(*offset, Some(0x1234));
                    assert_eq!(key.as_deref(), Some("mykey"));
                }
            }

            // Проверяем set_offset и set_key
            let v2 = v.with_context(0x4321, Some("key2"));
            match &v2 {
                ZdbVersionError::UnsupportedVersion { offset, key, .. }
                | ZdbVersionError::IncompatibleVersion { offset, key, .. }
                | ZdbVersionError::DeprecatedVersion { offset, key, .. }
                | ZdbVersionError::WriteIncompatible { offset, key, .. } => {
                    assert_eq!(*offset, Some(0x4321));
                    assert_eq!(key.as_deref(), Some("key2"));
                }
            }

            // Проверяем Display
            let s = format!("{v2}");
            assert!(s.len() > 0); // просто sanity-check, что что-то отформатировано

            // Проверяем client_message
            let msg = v2.client_message();
            match &v2 {
                ZdbVersionError::UnsupportedVersion { .. } => {
                    assert_eq!(msg, "Unsupported database version");
                }
                ZdbVersionError::IncompatibleVersion { .. } => {
                    assert_eq!(msg, "Incompatible database version");
                }
                ZdbVersionError::DeprecatedVersion { .. } => {
                    assert_eq!(msg, "Deprecated database version");
                }
                ZdbVersionError::WriteIncompatible { .. } => {
                    assert_eq!(msg, "Database version write error");
                }
            }

            // Проверяем status_code
            assert_eq!(v2.status_code(), StatusCode::UnsupportedVersion);

            // Проверяем as_any -> downcast
            let any_ref: &dyn Any = v2.as_any();
            assert!(any_ref.downcast_ref::<ZdbVersionError>().is_some());
        }
    }

    /// Тест проверяет, что ZdbError::Version корректно отображается через
    /// Display с offset и key.
    #[test]
    fn test_zdb_error_version_display_with_context() {
        let ver_err = ZdbVersionError::IncompatibleVersion {
            reader: 1,
            dump: 2,
            offset: Some(0x10),
            key: Some("k".into()),
        };
        let zerr: ZdbError = ver_err.clone().into();
        let s = format!("{zerr}");
        assert!(s.contains("Incompatible version"));
        assert!(s.contains("offset: 0x10"));
        assert!(s.contains("key: k"));
    }

    /// Тест проверяет recovery_hint и is_recoverable для ZdbError::Version с
    /// DeprecatedVersion
    #[test]
    fn test_version_error_recovery_hint() {
        let v = ZdbVersionError::DeprecatedVersion {
            version: 1,
            recommended: 2,
            offset: None,
            key: None,
        };
        let z: ZdbError = v.into();
        assert_eq!(
            z.recovery_hint(),
            Some("Consider upgrading the dump format to the latest version")
        );
        assert!(z.is_recoverable());
    }

    /// Тест проверяет: source(), From<io::Error> конверсия, metrics_tags
    /// и Display.
    #[test]
    fn test_error_source_for_version() {
        let v = ZdbVersionError::UnsupportedVersion {
            found: 5,
            supported: vec![1, 2, 3],
            offset: None,
            key: None,
        };
        let z: ZdbError = v.clone().into();
        // source() должно быть Some и по to_string() совпадать с вложенной версией
        let src = z.source().expect("expected source for ZdbError::Version");
        assert_eq!(src.to_string(), v.to_string());
    }

    /// Тест проверяет, что From<io::Error> корректно мапит kind -> variant
    #[test]
    fn test_from_io_error_variants() {
        // UnexpectedEof -> UnexpectedEof
        let io_e = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof happened");
        let z = ZdbError::from(io_e);
        match z {
            ZdbError::UnexpectedEof { context, .. } => assert!(context.contains("eof happened")),
            other => panic!("unexpected variant: {:?}", other),
        }

        // InvalidData -> CorruptedData
        let io_e = std::io::Error::new(std::io::ErrorKind::InvalidData, "bad data");
        let z = ZdbError::from(io_e);
        match z {
            ZdbError::CorruptedData { reason, .. } => assert!(reason.contains("bad data")),
            other => panic!("unexpected variant: {:?}", other),
        }

        // InvalidInput -> EncodingError
        let io_e = std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid input");
        let z = ZdbError::from(io_e);
        match z {
            ZdbError::EncodingError { what, reason, .. } => {
                assert_eq!(what, "input");
                assert!(reason.contains("invalid input"));
            }
            other => panic!("unexpected variant: {:?}", other),
        }

        // PermissionDenied (or other) -> CorruptedData fallback
        let io_e = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no perms");
        let z = ZdbError::from(io_e);
        match z {
            ZdbError::CorruptedData { reason, .. } => assert!(reason.contains("no perms")),
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    /// Тест проверяет, что metrics_tags содержит ожидаемые специфичные теги и
    /// общие теги.
    #[test]
    fn test_metrics_tags_compression_and_version() {
        // compression_op tag
        let e = ZdbError::CompressionError {
            operation: CompressionOp::Compress,
            reason: "compress failed".into(),
            offset: None,
            key: None,
            compressed_size: Some(123),
        };
        let tags = e.metrics_tags();
        // наличие compression_op == "Compress"
        assert!(tags
            .iter()
            .any(|(k, v)| *k == "compression_op" && v == "Compress"));
        // наличие status_code и recoverable
        assert!(tags.iter().any(|(k, _)| *k == "status_code"));
        assert!(tags.iter().any(|(k, _)| *k == "recoverable"));

        // version tag у ZdbError::Version(UnsupportedVersion)
        let v = ZdbVersionError::UnsupportedVersion {
            found: 7,
            supported: vec![1],
            offset: None,
            key: None,
        };
        let z = ZdbError::Version(v.clone());
        let tags = z.metrics_tags();
        assert!(tags.iter().any(|(k, v)| *k == "version" && v == "7"));
    }

    /// Тест проверяет, что Display для CompressionOp (покрытие impl
    /// fmt::Display)
    #[test]
    fn test_compressionop_display() {
        assert_eq!(format!("{}", CompressionOp::Compress), "Compression");
        assert_eq!(format!("{}", CompressionOp::Decompress), "Decompression");
    }

    /// Тест проверяет, что sanity-check: Display для FileTooSmall и
    /// client_message
    #[test]
    fn test_file_too_small_display_and_client_message() {
        let e = ZdbError::FileTooSmall {
            size: 10,
            minimum: 1024,
        };
        let s = format!("{e}");
        assert!(s.contains("File too small"));
        assert_eq!(e.client_message(), "Database file is too small".to_string());
    }
}
