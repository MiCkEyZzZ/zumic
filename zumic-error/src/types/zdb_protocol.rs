use std::any::Any;

use crate::{ErrorExt, StatusCode};

/// Ошибки версий ZDB
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZdbVersionError {
    /// Неподдерживаемая версия дампа
    UnsupportedVersion { found: u8, supported: Vec<u8> },
    /// Несовместимость версий
    IncompatibleVersion { reader: u8, dump: u8 },
    /// Устаревшая версия
    DeprecatedVersion { version: u8, recommended: u8 },
    /// Невозможно записать версию
    WriteIncompatible { writer: u8, target: u8 },
}

impl std::fmt::Display for ZdbVersionError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "Unsupported ZDB dump version: {found}. Supported versions: {supported:?}"
            ),
            Self::IncompatibleVersion { reader, dump } => write!(
                f,
                "Version incompatibility: reader {reader} cannot read dump version {dump}"
            ),
            Self::DeprecatedVersion {
                version,
                recommended,
            } => write!(
                f,
                "Deprecated version {version} detected. Please upgrade to {recommended}"
            ),
            Self::WriteIncompatible { writer, target } => write!(
                f,
                "Cannot write version {target} dump using {writer} writer"
            ),
        }
    }
}

impl std::error::Error for ZdbVersionError {}

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

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;

    /// Тест проверяет Display для UnsupportedVersion и ожидаемый формат строки.
    #[test]
    fn test_display_unsupported_version() {
        let err = ZdbVersionError::UnsupportedVersion {
            found: 3,
            supported: vec![1, 2],
        };
        let s = format!("{}", err);
        assert_eq!(
            s,
            "Unsupported ZDB dump version: 3. Supported versions: [1, 2]"
        );
    }

    /// Тест проверяет Display для IncompatibleVersion и ожидаемый формат
    /// строки.
    #[test]
    fn test_display_incompatible_version() {
        let err = ZdbVersionError::IncompatibleVersion { reader: 1, dump: 2 };
        let s = format!("{}", err);
        assert_eq!(
            s,
            "Version incompatibility: reader 1 cannot read dump version 2"
        );
    }

    /// Тест проверяет Display для DeprecatedVersion и ожидаемый формат строки.
    #[test]
    fn test_display_deprecated_version() {
        let err = ZdbVersionError::DeprecatedVersion {
            version: 1,
            recommended: 2,
        };
        let s = format!("{}", err);
        assert_eq!(s, "Deprecated version 1 detected. Please upgrade to 2");
    }

    /// Тест проверяет Display для WriteIncompatible и ожидаемый формат строки.
    #[test]
    fn test_display_write_incompatible() {
        let err = ZdbVersionError::WriteIncompatible {
            writer: 10,
            target: 20,
        };
        let s = format!("{}", err);
        assert_eq!(s, "Cannot write version 20 dump using 10 writer");
    }

    /// Тест проверяет соответствие client_message() ожидаемым сообщениям для
    /// всех вариантов.
    #[test]
    fn test_client_message_matches_variant() {
        let e1 = ZdbVersionError::UnsupportedVersion {
            found: 0,
            supported: vec![],
        };
        assert_eq!(
            e1.client_message(),
            "Unsupported database version".to_string()
        );

        let e2 = ZdbVersionError::IncompatibleVersion { reader: 0, dump: 0 };
        assert_eq!(
            e2.client_message(),
            "Incompatible database version".to_string()
        );

        let e3 = ZdbVersionError::DeprecatedVersion {
            version: 0,
            recommended: 0,
        };
        assert_eq!(
            e3.client_message(),
            "Deprecated database version".to_string()
        );

        let e4 = ZdbVersionError::WriteIncompatible {
            writer: 0,
            target: 0,
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
            },
            ZdbVersionError::IncompatibleVersion { reader: 1, dump: 2 },
            ZdbVersionError::DeprecatedVersion {
                version: 1,
                recommended: 2,
            },
            ZdbVersionError::WriteIncompatible {
                writer: 1,
                target: 2,
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
        let err = ZdbVersionError::IncompatibleVersion { reader: 7, dump: 8 };
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
        };
        let _err_ref: &dyn std::error::Error = &err; // компилируется только
                                                     // если impl std::error::Error
                                                     // существует
    }

    /// Тест проверяет, что as_any() позволяет выполнить downcast_ref к
    /// ZdbVersionError и затем проверить конкретный вариант и поля.
    #[test]
    fn test_as_any_downcast_and_match_variant_fields() {
        let err = ZdbVersionError::IncompatibleVersion { reader: 7, dump: 8 };
        let any_ref: &dyn Any = err.as_any();
        let down = any_ref
            .downcast_ref::<ZdbVersionError>()
            .expect("downcast to ZdbVersionError failed");

        // благодаря PartialEq можно сравнить напрямую
        assert_eq!(
            down,
            &ZdbVersionError::IncompatibleVersion { reader: 7, dump: 8 }
        );

        // либо распарсить вручную и проверить поля:
        if let ZdbVersionError::IncompatibleVersion { reader, dump } = down {
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
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
