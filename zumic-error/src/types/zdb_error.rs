use std::any::Any;

use crate::{ErrorExt, StatusCode};

/// Основная ошибка ZDB дампа с контекстом для диагностики.
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

/// Тип операции сжатия для контекста ошибки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionOp {
    Compress,
    Decompress,
}

/// Ошибки версий ZDB
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

impl std::fmt::Display for ZdbError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::CorruptedData {
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
            Self::InvalidTag {
                tag,
                offset,
                key,
                valid_tags,
            } => {
                write!(f, "Invalid tag 0x{tag:02X} (valid: {valid_tags:?})")?;
                write_context(f, *offset, key.as_deref())
            }
            Self::CompressionError {
                operation,
                reason,
                offset,
                key,
                compressed_size,
            } => {
                write!(f, "{operation:?} error: {reason}")?;
                if let Some(size) = compressed_size {
                    write!(f, " (size: {size} bytes)")?;
                }
                write_context(f, *offset, key.as_deref())
            }
            Self::UnexpectedEof {
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
            Self::SizeLimit {
                what,
                size,
                limit,
                offset,
                key,
            } => {
                write!(f, "{what} size {size} exceeds limit {limit} bytes")?;
                write_context(f, *offset, key.as_deref())
            }
            Self::InvalidMagic { expected, got } => {
                write!(
                    f,
                    "Invalid magic number: expected {expected:?}, got {got:?}",
                )
            }
            Self::CrcMismatch {
                computed,
                recorded,
                offset,
            } => {
                write!(
                    f,
                    "CRC mismatch: computed 0x{computed:08X}, recorded 0x{recorded:08X}",
                )?;
                write_context(f, *offset, None)
            }
            Self::Version(v) => write!(f, "{v}"),
            Self::ParseError {
                structure,
                reason,
                offset,
                key,
            } => {
                write!(f, "Failed to parse {structure}: {reason}")?;
                write_context(f, *offset, key.as_deref())
            }
            Self::EncodingError { what, reason, key } => {
                write!(f, "Encoding error for {what}: {reason}")?;
                if let Some(k) = key {
                    write!(f, " (key: {k})")?;
                }
                Ok(())
            }
            Self::FileTooSmall { size, minimum } => {
                write!(f, "File too small: {size} bytes (minimum: {minimum} bytes)",)
            }
        }
    }
}

impl std::fmt::Display for ZdbVersionError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion {
                found,
                supported,
                offset,
                key,
            } => {
                write!(f, "Unsupported version {found} (supported: {supported:?})",)?;
                write_context(f, *offset, key.as_deref())
            }
            Self::IncompatibleVersion {
                reader,
                dump,
                offset,
                key,
            } => {
                write!(
                    f,
                    "Incompatible version: reader v{reader} cannot read dump v{dump}",
                )?;
                write_context(f, *offset, key.as_deref())
            }
            Self::DeprecatedVersion {
                version,
                recommended,
                offset,
                key,
            } => {
                write!(
                    f,
                    "Deprecated version {version} (recommended: v{recommended})",
                )?;
                write_context(f, *offset, key.as_deref())
            }
            Self::WriteIncompatible {
                writer,
                target,
                offset,
                key,
            } => {
                write!(f, "Cannot write version {target} using writer v{writer}",)?;
                write_context(f, *offset, key.as_deref())
            }
        }
    }
}

/// Вспомогательная функция для форматирования контекста (offset, key).
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
            Self::CompressionError { .. } => "Compression/decompression failed".to_string(),
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
        let mut msg = format!("{:?}", self);
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

impl From<std::io::Error> for ZdbError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match e.kind() {
            ErrorKind::UnexpectedEof => ZdbError::UnexpectedEof {
                context: e.to_string(),
                offset: None,
                key: None,
                expected_bytes: None,
                got_bytes: None,
            },
            ErrorKind::InvalidData => ZdbError::CorruptedData {
                reason: e.to_string(),
                offset: None,
                key: None,
                expected: None,
                got: None,
            },
            ErrorKind::InvalidInput => ZdbError::ParseError {
                structure: "I/O".to_string(),
                reason: e.to_string(),
                offset: None,
                key: None,
            },
            _ => ZdbError::ParseError {
                structure: "I/O".to_string(),
                reason: e.to_string(),
                offset: None,
                key: None,
            },
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
