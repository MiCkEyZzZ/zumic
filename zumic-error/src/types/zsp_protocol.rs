use std::{any::Any, io};

use crate::{ErrorExt, StatusCode};

/// Ошибка кодирования ZSP.
#[derive(Debug)]
pub enum ZspEncodeError {
    /// Невалидные данные для кодирования
    InvalidData { reason: String },
    /// Невалидное состояние энкодера
    InvalidState { reason: String },
    /// Превышен лимит размера
    SizeLimit {
        data_type: String,
        current: usize,
        max: usize,
    },
    /// Превышен лимит глубины вложенности
    DepthLimit { current: usize, max: usize },
    /// Ошибка компрессии
    CompressionError { reason: String },
    /// Ошибка сериализации (вложенный enum)
    SerializationError { inner: ZspSerializationError },
    /// Строка содержит недопустимые символы
    InvalidStringFormat { reason: String },
    /// Ошибка I/O при кодировании — сохраняем оригинальный std::io::Error как
    /// источник
    IoError { source: io::Error },
}

/// Ошибка декодирования ZSP.
#[derive(Debug, Clone)]
pub enum ZspDecodeError {
    /// Невалидное состояние декодера
    InvalidState { reason: String },
    /// Невалидные данные
    InvalidData { reason: String },
    /// Неожиданный конец данных
    UnexpectedEof { context: String },
    /// Невалидная UTF-8 кодировка
    InvalidUtf8 { context: String },
    /// Невалидное целое число
    InvalidInteger { context: String },
    /// Невалидное число с плавающей точкой
    InvalidFloat { context: String },
    /// Невалидное булево значение
    InvalidBoolean { context: String },
    /// Превышена максимальная глубина массивов
    MaxArrayDepthExceeded { depth: usize },
    /// Невалидный тип фрейма
    InvalidFrameType { type_id: u8 },
    /// Превышен лимит размера
    SizeLimit {
        data_type: String,
        current: usize,
        max: usize,
    },
    /// Превышен лимит глубины
    DepthLimit { current: usize, max: usize },
    /// Повреждение данных
    Corruption {
        position: usize,
        expected: String,
        found: String,
    },
}

/// Ошибка парсинга команд ZSP.
#[derive(Debug, Clone)]
pub enum ZspParserError {
    /// Неизвестная команда
    UnknownCommand { command: String },
    /// Неверное количество аргументов
    WrongArgCount {
        command: String,
        expected: usize,
        got: usize,
    },
    /// Специфичная ошибка для MSET (как в старой версии)
    MSetWrongArgCount,
    /// Команда должна быть строкой
    CommandMustBeString,
    /// Ожидался массив как формат команды
    ExpectedArray,
    /// Невалидный ключ
    InvalidKey { command: String, reason: String },
    /// Невалидная UTF-8 кодировка
    InvalidUtf8,
    /// Команда требует согласования версии
    RequiresVersionNegotiation { command: String },
    /// Неожиданный handshake фрейм
    UnexpectedHandshake { context: String },
    /// Команда не реализована
    CommandNotImplemented { command: String },
    /// Расширенные типы не реализованы
    ExtendedTypeNotImplemented,
    /// Невалидный тип значения
    InvalidValueType { command: String, reason: String },
}

/// Сериализационные ошибки ZSP (вынесены в отдельный enum).
#[derive(Debug, Clone)]
pub enum ZspSerializationError {
    /// Требуется согласование версии перед сериализацией
    RequiresVersionNegotiation,
    /// Ошибка JSON-сериализации
    JsonError { reason: String },
    /// Ошибка конвертации данных перед сериализацией
    ConversionError { reason: String },
    /// Недопустимый тип данных для сериализации
    InvalidDataType { type_name: String },
}

impl ErrorExt for ZspEncodeError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidData { .. }
            | Self::InvalidState { .. }
            | Self::InvalidStringFormat { .. } => StatusCode::EncodingError,
            Self::SizeLimit { .. } => StatusCode::SizeLimit,
            Self::DepthLimit { .. } => StatusCode::DepthLimit,
            Self::CompressionError { .. } => StatusCode::CompressionFailed,
            Self::SerializationError { .. } => StatusCode::SerializationFailed,
            Self::IoError { .. } => StatusCode::Io,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::InvalidData { .. } => "Invalid data format".to_string(),
            Self::InvalidState { .. } => "Protocol encoding error".to_string(),
            Self::SizeLimit { data_type, .. } => format!("{data_type} size limit exceeded"),
            Self::DepthLimit { .. } => "Data structure too deeply nested".to_string(),
            Self::CompressionError { .. }
            | Self::SerializationError { .. }
            | Self::IoError { .. } => "Internal server error".to_string(),
            Self::InvalidStringFormat { .. } => "Invalid string format".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "zsp_encode".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        if let Self::SizeLimit { data_type, .. } = self {
            tags.push(("data_type", data_type.clone()));
        }

        if let Self::SerializationError { inner } = self {
            let kind = match inner {
                ZspSerializationError::RequiresVersionNegotiation => "requires_version",
                ZspSerializationError::JsonError { .. } => "json",
                ZspSerializationError::ConversionError { .. } => "conversion",
                ZspSerializationError::InvalidDataType { .. } => "invalid_type",
            };
            tags.push(("serialization_kind", kind.to_string()));
        }

        tags
    }
}

impl ErrorExt for ZspDecodeError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidState { .. }
            | Self::InvalidData { .. }
            | Self::InvalidUtf8 { .. }
            | Self::InvalidInteger { .. }
            | Self::InvalidFloat { .. }
            | Self::InvalidBoolean { .. }
            | Self::InvalidFrameType { .. } => StatusCode::DecodingError,
            Self::UnexpectedEof { .. } => StatusCode::UnexpectedEof,
            Self::MaxArrayDepthExceeded { .. } | Self::DepthLimit { .. } => StatusCode::DepthLimit,
            Self::SizeLimit { .. } => StatusCode::SizeLimit,
            Self::Corruption { .. } => StatusCode::CorruptedData,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::InvalidState { .. }
            | Self::InvalidData { .. }
            | Self::InvalidUtf8 { .. }
            | Self::InvalidInteger { .. }
            | Self::InvalidFloat { .. }
            | Self::InvalidBoolean { .. }
            | Self::InvalidFrameType { .. } => "Invalid protocol data".to_string(),
            Self::UnexpectedEof { .. } => "Incomplete data received".to_string(),
            Self::MaxArrayDepthExceeded { .. } | Self::DepthLimit { .. } => {
                "Data structure too deeply nested".to_string()
            }
            Self::SizeLimit { data_type, .. } => format!("{data_type} size limit exceeded"),
            Self::Corruption { .. } => "Corrupted data received".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "zsp_decode".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::SizeLimit { data_type, .. } => {
                tags.push(("data_type", data_type.clone()));
            }
            Self::InvalidFrameType { type_id } => {
                tags.push(("frame_type", format!("0x{type_id:02x}")));
            }
            _ => {}
        }

        tags
    }
}

impl ErrorExt for ZspParserError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::UnknownCommand { .. } => StatusCode::InvalidCommand,
            Self::WrongArgCount { .. } | Self::MSetWrongArgCount => StatusCode::InvalidArgs,
            Self::CommandMustBeString | Self::ExpectedArray | Self::InvalidUtf8 => {
                StatusCode::ParseError
            }
            Self::InvalidKey { .. } => StatusCode::InvalidKey,
            Self::RequiresVersionNegotiation { .. } => StatusCode::VersionMismatch,
            Self::UnexpectedHandshake { .. } => StatusCode::ProtocolError,
            Self::CommandNotImplemented { .. } | Self::ExtendedTypeNotImplemented => {
                StatusCode::NotImplemented
            }
            Self::InvalidValueType { .. } => StatusCode::InvalidValue,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::UnknownCommand { command } => format!("Unknown command: {command}"),
            Self::WrongArgCount {
                command, expected, ..
            } => {
                format!("Wrong number of arguments for {command}: expected {expected}")
            }
            Self::MSetWrongArgCount => {
                "Wrong number of arguments for MSET: arguments must be key-value pairs".to_string()
            }
            Self::CommandMustBeString => "Command must be a string".to_string(),
            Self::ExpectedArray => "Invalid command format".to_string(),
            Self::InvalidKey { .. } => "Invalid key".to_string(),
            Self::InvalidUtf8 => "Invalid UTF-8 encoding".to_string(),
            Self::RequiresVersionNegotiation { .. } => {
                "Protocol version negotiation required".to_string()
            }
            Self::UnexpectedHandshake { .. } => "Unexpected protocol handshake".to_string(),
            Self::CommandNotImplemented { command } => {
                format!("Command not implemented: {command}")
            }
            Self::ExtendedTypeNotImplemented => "Feature not implemented".to_string(),
            Self::InvalidValueType { .. } => "Invalid value type".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "zsp_parser".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::UnknownCommand { command }
            | Self::WrongArgCount { command, .. }
            | Self::InvalidKey { command, .. }
            | Self::RequiresVersionNegotiation { command }
            | Self::CommandNotImplemented { command }
            | Self::InvalidValueType { command, .. } => {
                tags.push(("command", command.clone()));
            }
            Self::MSetWrongArgCount => {
                tags.push(("command", "MSET".to_string()));
            }
            _ => {}
        }

        tags
    }
}

impl std::fmt::Display for ZspEncodeError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::InvalidData { reason } => write!(f, "Invalid data for encoding: {reason}"),
            Self::InvalidState { reason } => write!(f, "Invalid encoder state: {reason}"),
            Self::SizeLimit {
                data_type,
                current,
                max,
            } => write!(f, "Size limit exceeded for {data_type}: {current} > {max}"),
            Self::DepthLimit { current, max } => {
                write!(f, "Depth limit exceeded: {current} > {max}")
            }
            Self::CompressionError { reason } => write!(f, "Compression error: {reason}"),
            Self::SerializationError { inner } => write!(f, "Serialization error: {inner}"),
            Self::InvalidStringFormat { reason } => {
                write!(f, "Invalid string format: {reason}")
            }
            Self::IoError { source } => write!(f, "I/O error during encoding: {source}"),
        }
    }
}

impl std::fmt::Display for ZspDecodeError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::InvalidState { reason } => write!(f, "Invalid decoder state: {reason}"),
            Self::InvalidData { reason } => write!(f, "Invalid data: {reason}"),
            Self::UnexpectedEof { context } => write!(f, "Unexpected EOF: {context}"),
            Self::InvalidUtf8 { context } => write!(f, "Invalid UTF-8 encoding: {context}"),
            Self::InvalidInteger { context } => write!(f, "Invalid integer: {context}"),
            Self::InvalidFloat { context } => write!(f, "Invalid float: {context}"),
            Self::InvalidBoolean { context } => write!(f, "Invalid boolean: {context}"),
            Self::MaxArrayDepthExceeded { depth } => {
                write!(f, "Maximum array depth exceeded: {depth}")
            }
            Self::InvalidFrameType { type_id } => write!(f, "Invalid frame type: 0x{type_id:02x}"),
            Self::SizeLimit {
                data_type,
                current,
                max,
            } => write!(f, "Size limit exceeded for {data_type}: {current} > {max}"),
            Self::DepthLimit { current, max } => {
                write!(f, "Depth limit exceeded: {current} > {max}")
            }
            Self::Corruption {
                position,
                expected,
                found,
            } => write!(
                f,
                "Data corruption at position {position}: expected {expected}, found {found}"
            ),
        }
    }
}

impl std::fmt::Display for ZspParserError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::UnknownCommand { command } => write!(f, "Unknown command: {command}"),
            Self::WrongArgCount {
                command,
                expected,
                got,
            } => write!(
                f,
                "Wrong number of arguments for {command}: expected {expected}, got {got}"
            ),
            Self::MSetWrongArgCount => write!(
                f,
                "Wrong number of arguments for MSET: arguments must be key-value pairs"
            ),
            Self::CommandMustBeString => write!(f, "Command must be a string"),
            Self::ExpectedArray => write!(f, "Expected array as command format"),
            Self::InvalidKey { command, reason } => {
                write!(f, "Invalid key for command {command}: {reason}")
            }
            Self::InvalidUtf8 => write!(f, "Invalid UTF-8 encoding"),
            Self::RequiresVersionNegotiation { command } => {
                write!(f, "Command '{command}' requires version negotiation")
            }
            Self::UnexpectedHandshake { context } => {
                write!(f, "Unexpected handshake frame: {context}")
            }
            Self::CommandNotImplemented { command } => {
                write!(f, "Command '{command}' not implemented yet")
            }
            Self::ExtendedTypeNotImplemented => write!(f, "Extended type frames not implemented"),
            Self::InvalidValueType { command, reason } => {
                write!(f, "Invalid value type for {command}: {reason}")
            }
        }
    }
}

impl std::fmt::Display for ZspSerializationError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::RequiresVersionNegotiation => {
                write!(f, "Serialization requires version negotiation")
            }
            Self::JsonError { reason } => write!(f, "JSON serialization error: {reason}"),
            Self::ConversionError { reason } => write!(f, "Data conversion error: {reason}"),
            Self::InvalidDataType { type_name } => {
                write!(f, "Invalid data type for serialization: {type_name}")
            }
        }
    }
}

impl std::error::Error for ZspEncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IoError { source } => Some(source),
            _ => None,
        }
    }
}

impl std::error::Error for ZspDecodeError {}
impl std::error::Error for ZspParserError {}

impl std::error::Error for ZspSerializationError {}

impl From<ZspSerializationError> for ZspEncodeError {
    fn from(inner: ZspSerializationError) -> Self {
        ZspEncodeError::SerializationError { inner }
    }
}

impl From<io::Error> for ZspEncodeError {
    fn from(source: io::Error) -> Self {
        ZspEncodeError::IoError { source }
    }
}

#[cfg(test)]
mod tests {
    use std::{any::Any, io};

    use super::*;

    /// Тест проверяет Display для ZspEncodeError::InvalidData.
    #[test]
    fn test_display_encode_invalid_data() {
        let err = ZspEncodeError::InvalidData {
            reason: "bad payload".to_string(),
        };
        assert_eq!(format!("{}", err), "Invalid data for encoding: bad payload");
    }

    /// Тест проверяет Display для ZspEncodeError::SizeLimit.
    #[test]
    fn test_display_encode_size_limit() {
        let err = ZspEncodeError::SizeLimit {
            data_type: "blob".to_string(),
            current: 10,
            max: 5,
        };
        assert_eq!(format!("{}", err), "Size limit exceeded for blob: 10 > 5");
    }

    /// Тест проверяет конвертацию ZspSerializationError -> ZspEncodeError,
    /// Display и метки metrics_tags.
    #[test]
    fn test_serialization_error_conversion_and_metrics() {
        let inner = ZspSerializationError::JsonError {
            reason: "invalid json".to_string(),
        };
        let enc: ZspEncodeError = ZspEncodeError::from(inner.clone());
        // Display должен включать Display вложенного inner
        assert_eq!(
            format!("{}", enc),
            "Serialization error: JSON serialization error: invalid json"
        );

        // metrics_tags должен содержать serialization_kind = "json"
        let tags = enc.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| *k == "serialization_kind" && v == "json"));
    }

    /// Тест проверяет From<io::Error> для ZspEncodeError и что Error::source
    /// возвращает оригинальную ошибку.
    #[test]
    fn test_from_from_io_error_and_source() {
        let io_err = io::Error::new(io::ErrorKind::Other, "disk failure");
        let enc = ZspEncodeError::from(io_err);
        // Display содержит текст io error
        assert!(format!("{}", enc).contains("I/O error during encoding: disk failure"));
        // source() вызываем через квалифицированное имя трейта — это надёжно и
        // устраняет E0599
        let src = std::error::Error::source(&enc).expect("expected source");
        assert_eq!(src.to_string(), "disk failure");
    }

    /// Тест проверяет Display и статус код для ZspDecodeError::Corruption.
    #[test]
    fn test_decode_corruption_display_and_status() {
        let d = ZspDecodeError::Corruption {
            position: 5,
            expected: "A".to_string(),
            found: "B".to_string(),
        };
        assert_eq!(
            format!("{}", d),
            "Data corruption at position 5: expected A, found B"
        );
        assert_eq!(d.status_code(), StatusCode::CorruptedData);
    }

    /// Тест проверяет metrics_tags для ZspDecodeError::SizeLimit (проверка
    /// data_type).
    #[test]
    fn test_decode_size_limit_metrics() {
        let d = ZspDecodeError::SizeLimit {
            data_type: "payload".to_string(),
            current: 12,
            max: 10,
        };
        let tags = d.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| *k == "data_type" && v == "payload"));
    }

    /// Тест проверяет client_message, status_code и metrics_tags для
    /// ZspParserError::MSetWrongArgCount.
    #[test]
    fn test_parser_mset_wrong_argcount_metrics_and_client_message() {
        let p = ZspParserError::MSetWrongArgCount;
        assert_eq!(
            p.client_message(),
            "Wrong number of arguments for MSET: arguments must be key-value pairs"
        );
        assert_eq!(p.status_code(), StatusCode::InvalidArgs);

        let tags = p.metrics_tags();
        assert!(tags.iter().any(|(k, v)| *k == "command" && v == "MSET"));
    }

    /// Тест проверяет as_any() и возможность downcast_ref для ZspEncodeError.
    #[test]
    fn test_as_any_downcast_encode() {
        let err = ZspEncodeError::InvalidState {
            reason: "broken".to_string(),
        };
        let any_ref: &dyn Any = (&err).as_any();
        assert!(any_ref.downcast_ref::<ZspEncodeError>().is_some());
    }

    /// Тест проверяет as_any() и возможность downcast_ref для ZspDecodeError.
    #[test]
    fn test_as_any_downcast_decode() {
        let err = ZspDecodeError::InvalidInteger {
            context: "not an int".to_string(),
        };
        let any_ref: &dyn Any = (&err).as_any();
        assert!(any_ref.downcast_ref::<ZspDecodeError>().is_some());
    }

    /// Тест проверяет as_any() и возможность downcast_ref для ZspParserError.
    #[test]
    fn test_as_any_downcast_parser() {
        let err = ZspParserError::UnknownCommand {
            command: "FOO".to_string(),
        };
        let any_ref: &dyn Any = (&err).as_any();
        assert!(any_ref.downcast_ref::<ZspParserError>().is_some());
    }

    /// Тест проверяет From<ZspSerializationError> -> ZspEncodeError, Display и
    /// serialization_kind в metrics_tags.
    #[test]
    fn test_from_zsp_serialization_error_conversion() {
        let inner = ZspSerializationError::ConversionError {
            reason: "bad conversion".to_string(),
        };
        let enc = ZspEncodeError::from(inner.clone());
        assert_eq!(
            format!("{}", enc),
            "Serialization error: Data conversion error: bad conversion"
        );
        let tags = enc.metrics_tags();
        assert!(tags
            .iter()
            .any(|(k, v)| *k == "serialization_kind" && v == "conversion"));
        assert_eq!(enc.status_code(), StatusCode::SerializationFailed);
    }
}
