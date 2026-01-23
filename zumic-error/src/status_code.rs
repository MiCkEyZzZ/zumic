use std::fmt;

use num_enum::TryFromPrimitive;
#[cfg(feature = "serde_repr")]
use serde_repr::{Deserialize_repr, Serialize_repr};
#[cfg(feature = "strum")]
use strum_macros::{AsRefStr, EnumIter};

/// Коды статуса для категоризации ошибок.
///
/// # Диапазоны:
/// - 0xxx: Успех
/// - 1xxx: Общие ошибки
/// - 2xxx: Ошибки данных
/// - 3xxx: Авторизация / Разрешения
/// - 4xxx: Ограничения по частоте (rate limiting)
/// - 5xxx: Хранилище
/// - 6xxx: Сеть / IO
/// - 7xxx: Кластерные ошибки
/// - 8xxx: Протокольные ошибки
///
/// # Реализация:
/// - `num_enum::TryFromPrimitive` даёт нативную реализацию `TryFrom<u32>`
///   (полезно для wire-protocol).
/// - опционально: `strum` для `AsRefStr`/`EnumIter` (feature = "strum").
/// - опционально: `serde_repr` для сериализации в виде числового значения
///   (feature = "serde_repr").
#[cfg_attr(feature = "strum", derive(AsRefStr, EnumIter))]
#[cfg_attr(feature = "serde_repr", derive(Serialize_repr, Deserialize_repr))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
#[non_exhaustive]
pub enum StatusCode {
    // === 0xxx: Успех ===
    Success = 0,

    // === 1xxx: Общие ошибки ===
    Unknown = 1000,
    Unsupported = 1001,
    Unexpected = 1002,
    Internal = 1003,
    InvalidArgs = 1004,
    NotImplemented = 1005,

    // === 2xxx: Ошибки данных ===
    NotFound = 2000,
    AlreadyExists = 2001,
    TypeError = 2002,
    InvalidKey = 2003,
    InvalidValue = 2004,
    ExpiredKey = 2005,
    IndexOutOfBounds = 2006,
    WrongType = 2007,
    InvalidOperation = 2008,
    InvalidData = 2009,

    // === 3xxx: Авторизация/Разрешение ===
    AuthFailed = 3000,
    PermissionDenied = 3001,
    SessionExpired = 3002,
    InvalidToken = 3003,
    UserNotFound = 3004,
    UserExists = 3005,
    InvalidCredentials = 3006,
    PasswordHashFailed = 3007,
    TooManyAttempts = 3008,
    Unauthorized = 3009,

    // === 4xxx: Ограничение скорости ===
    RateLimited = 4000,
    QuotaExceeded = 4001,
    TooManyConnections = 4002,
    SubscriberLimitExceeded = 4003,

    // === 5xxx: Хранилище ===
    StorageUnavailable = 5000,
    DiskFull = 5001,
    CorruptedData = 5002,
    SerializationFailed = 5003,
    DeserializationFailed = 5004,
    CompressionFailed = 5005,
    WrongShard = 5006,
    LockError = 5007,

    // === 6xxx: Сеть/IO ===
    Io = 6000,
    ConnectionClosed = 6001,
    Timeout = 6002,
    ProtocolError = 6003,
    ConnectionFailed = 6004,
    ReadTimeout = 6005,
    WriteTimeout = 6006,
    UnexpectedEof = 6007,

    // === 7xxx: Кластер ===
    ClusterDown = 7000,
    MovedSlot = 7001,
    CrossSlot = 7002,
    MigrationActive = 7003,
    InvalidShard = 7004,
    InvalidSlot = 7005,
    RebalanceFailed = 7006,

    // === 8xxx: Протокол ===
    InvalidFrame = 8000,
    InvalidCommand = 8001,
    UnsupportedVersion = 8002,
    VersionMismatch = 8003,
    InvalidUtf8 = 8004,
    InvalidInteger = 8005,
    InvalidFloat = 8006,
    SizeLimit = 8007,
    DepthLimit = 8008,
    ParseError = 8009,
    EncodingError = 8010,
    DecodingError = 8011,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl StatusCode {
    /// Числовое представление кода статуса.
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// Пытается получить вариант `StatusCode` из `u32`.
    ///
    /// Использует `TryFrom<u32>` из `num_enum`; возвращает `None`, если
    /// значение не соответствует ни одному варианту.
    pub fn from_u32(v: u32) -> Option<Self> {
        Self::try_from(v).ok()
    }

    /// Возвращает `true`, если ошибку с этим кодом имеет смысл пытаться
    /// повторить (retryable).
    ///
    /// Под «повторить» обычно понимается автоматическая повторная попытка
    /// операции.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout
                | Self::ReadTimeout
                | Self::WriteTimeout
                | Self::StorageUnavailable
                | Self::RateLimited
                | Self::TooManyConnections
                | Self::ConnectionFailed
                | Self::ClusterDown
        )
    }

    /// Вернёт `true`, если переданный `code` означает успешный результат.
    pub fn is_success(code: u32) -> bool {
        Self::Success as u32 == code
    }

    /// Является ли код ошибкой со стороны клиента — проблема в запросе или
    /// данных.
    ///
    /// В большинстве случаев клиентские ошибки лежат в диапазоне `2xxx..4xxx`.
    /// Значение `InvalidArgs` (1004) семантически относится к клиентским
    /// ошибкам и учитывается явно.
    pub fn is_client_error(&self) -> bool {
        let c = self.code();
        if (2000..=4999).contains(&c) {
            return true;
        }
        matches!(self, Self::InvalidArgs)
    }

    /// Является ли код ошибкой сервера — внутренняя или инфраструктурная
    /// ошибка.
    ///
    /// Обычно это диапазоны `1xxx` и `5xxx..7xxx`.
    pub fn is_server_error(&self) -> bool {
        let c = self.code();
        matches!(c, 1000..=1999 | 5000..=7999)
    }

    /// Ошибка протокола или парсинга (диапазон 8xxx).
    pub fn is_protocol_error(&self) -> bool {
        (8000..=8999).contains(&self.code())
    }

    /// Требуется ли логировать как критическую ошибку.
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            Self::Internal
                | Self::CorruptedData
                | Self::DiskFull
                | Self::StorageUnavailable
                | Self::ClusterDown
        )
    }

    /// Рекомендуемый уровень логирования для данного кода.
    pub fn log_level(&self) -> LogLevel {
        match self {
            Self::Success => LogLevel::Trace,
            Self::NotFound | Self::AlreadyExists => LogLevel::Debug,
            Self::InvalidArgs
            | Self::TypeError
            | Self::InvalidKey
            | Self::InvalidValue
            | Self::InvalidData
            | Self::AuthFailed
            | Self::PermissionDenied
            | Self::Unauthorized => LogLevel::Info,
            Self::RateLimited | Self::Timeout | Self::ConnectionClosed => LogLevel::Warn,
            Self::Internal
            | Self::CorruptedData
            | Self::DiskFull
            | Self::StorageUnavailable
            | Self::ClusterDown => LogLevel::Error,
            _ => LogLevel::Warn,
        }
    }

    /// HTTP-статус, соответствующий коду статуса.
    ///
    /// Используется для маппинга кодов ошибок на HTTP-ответы при работе через
    /// REST/HTTP.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Success => 200,
            Self::NotFound => 404,
            Self::AlreadyExists => 409,
            Self::InvalidArgs
            | Self::TypeError
            | Self::InvalidKey
            | Self::InvalidValue
            | Self::InvalidData
            | Self::InvalidCommand => 400,
            Self::AuthFailed
            | Self::InvalidCredentials
            | Self::SessionExpired
            | Self::InvalidToken
            | Self::Unauthorized => 401,
            Self::PermissionDenied => 403,
            Self::RateLimited | Self::TooManyAttempts => 429,
            Self::Timeout | Self::ReadTimeout | Self::WriteTimeout => 408,
            Self::NotImplemented | Self::Unsupported => 501,
            Self::StorageUnavailable | Self::ClusterDown => 503,
            _ => 500,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для StatusCode
////////////////////////////////////////////////////////////////////////////////

impl From<StatusCode> for u32 {
    fn from(c: StatusCode) -> Self {
        c.code()
    }
}

impl fmt::Display for StatusCode {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        // Если включён feature "strum", используем human-readable имя (AsRefStr).
        // Иначе — Debug-имя.
        #[cfg(feature = "strum")]
        {
            write!(f, "{} ({})", self.as_ref(), self.code())
        }
        #[cfg(not(feature = "strum"))]
        {
            write!(f, "{:?} ({})", self, self.code())
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет, что retryable-коды помечаются корректно.
    #[test]
    fn test_retryable() {
        assert!(StatusCode::Timeout.is_retryable());
        assert!(StatusCode::RateLimited.is_retryable());
        assert!(!StatusCode::InvalidArgs.is_retryable());
        assert!(!StatusCode::NotFound.is_retryable());
    }

    /// Тест проверяет разделение клиентских и серверных ошибок.
    #[test]
    fn test_client_vs_server() {
        assert!(StatusCode::InvalidArgs.is_client_error());
        assert!(StatusCode::AuthFailed.is_client_error());
        assert!(StatusCode::Internal.is_server_error());
        assert!(StatusCode::StorageUnavailable.is_server_error());
    }

    /// Тест проверяет соответствие кодов HTTP-статусам.
    #[test]
    fn test_http_mapping() {
        assert_eq!(StatusCode::NotFound.http_status(), 404);
        assert_eq!(StatusCode::AuthFailed.http_status(), 401);
        assert_eq!(StatusCode::Unauthorized.http_status(), 401);
        assert_eq!(StatusCode::PermissionDenied.http_status(), 403);
        assert_eq!(StatusCode::RateLimited.http_status(), 429);
        assert_eq!(StatusCode::Internal.http_status(), 500);
    }

    /// Тест проверяет конвертацию через `TryFrom<u32>` и вспомогательную
    /// `from_u32`.
    #[test]
    fn test_from_try_from_u32() {
        let n = StatusCode::NotFound.code();
        assert_eq!(StatusCode::try_from(n).unwrap(), StatusCode::NotFound);
        assert!(StatusCode::from_u32(99999).is_none());
    }

    /// Тест проверяет получение числового представления и конвертацию
    /// `From<StatusCode> for u32`.
    #[test]
    fn test_code_and_into() {
        let c = StatusCode::NotFound;
        assert_eq!(c.code(), 2000);
        let n: u32 = c.into();
        assert_eq!(n, 2000);
        assert!(StatusCode::is_success(StatusCode::Success.code()));
        assert!(!StatusCode::is_success(StatusCode::NotFound.code()));
    }

    /// Тест проверяет определение протокольных/парсинг-ошибок (диапазон 8xxx).
    #[test]
    fn test_is_protocol_error() {
        assert!(StatusCode::InvalidCommand.is_protocol_error());
        assert!(!StatusCode::NotFound.is_protocol_error());
    }

    /// Тест проверяет, что критические ошибки помечаются корректно.
    #[test]
    fn test_is_critical() {
        assert!(StatusCode::Internal.is_critical());
        assert!(StatusCode::DiskFull.is_critical());
        assert!(!StatusCode::NotFound.is_critical());
    }

    /// Тест проверяет отображаемый уровень логирования для разных кодов.
    #[test]
    fn test_log_level_mappings() {
        assert_eq!(StatusCode::Success.log_level(), LogLevel::Trace);
        assert_eq!(StatusCode::NotFound.log_level(), LogLevel::Debug);
        assert_eq!(StatusCode::Internal.log_level(), LogLevel::Error);
    }

    /// Тест проверяет формат `Display` — строка должна содержать имя варианта и
    /// числовой код.
    #[test]
    fn test_display_contains_name_and_code() {
        let s = format!("{}", StatusCode::NotFound);
        assert!(
            s.contains("2000"),
            "Display must contain code 2000, got: {s}"
        );
        assert!(
            s.contains("NotFound"),
            "Display must contain variant name 'NotFound', got: {s}"
        );
    }

    /// Тест проверяет, что диапазоны серверных/клиентских ошибок определены
    /// верно.
    #[test]
    fn test_is_server_error_range() {
        // Unknown (1000) — считается серверной ошибкой (1xxx)
        assert!(StatusCode::Unknown.is_server_error());
        // NotFound (2000) — клиентская ошибка (2xxx)
        assert!(!StatusCode::NotFound.is_server_error());
    }

    /// Тест проверяет поведение http_status для варианта без явного маппинга
    /// (фоллбек -> 500).
    #[test]
    fn test_http_default_fallback() {
        // Unexpected (1002) нет в явных ветках -> попадает в `_ => 500`
        assert_eq!(StatusCode::Unexpected.http_status(), 500);
    }
}
