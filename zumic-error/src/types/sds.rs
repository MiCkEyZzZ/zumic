use std::any::Any;
use std::str::Utf8Error;

use crate::{ErrorExt, StatusCode};

/// Typed errors for Sds operations.
///
/// Covers UTF-8 decoding, numeric parsing, bounds checks, and size/memory
/// limits. All Sds operations return this error type instead of leaking
/// standard-library or ad-hoc error types.
#[derive(Debug, Clone)]
pub enum SdsError {
    /// String is not valid UTF-8.
    InvalidUtf8(Utf8Error),
    /// Cannot parse integer from Sds content.
    InvalidInteger,
    /// Cannot parse floating-point number from Sds content.
    InvalidFloat,
    /// Byte index is out of bounds.
    IndexOutOfBounds { len: usize, index: usize },
    /// Value exceeds the configured maximum length.
    ValueTooLarge { max: usize, actual: usize },
    /// Allocation failed — system is out of memory.
    MemoryExhausted,
}

impl std::fmt::Display for SdsError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::InvalidUtf8(err) => write!(f, "UTF-8 decoding failed: {err}"),
            Self::InvalidInteger => write!(f, "cannot parse integer from Sds"),
            Self::InvalidFloat => write!(f, "cannot parse float from Sds"),
            Self::IndexOutOfBounds { len, index } => {
                write!(f, "index {index} out of bounds for Sds of length {len}")
            }
            Self::ValueTooLarge { max, actual } => {
                write!(f, "value too large: {actual} bytes exceeds limit of {max}")
            }
            Self::MemoryExhausted => write!(f, "memory exhausted during Sds allocation"),
        }
    }
}

impl std::error::Error for SdsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidUtf8(err) => Some(err),
            _ => None,
        }
    }
}

impl ErrorExt for SdsError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidUtf8(_) => StatusCode::InvalidUtf8,
            Self::InvalidInteger => StatusCode::InvalidInteger,
            Self::InvalidFloat => StatusCode::InvalidFloat,
            Self::IndexOutOfBounds { .. } => StatusCode::IndexOutOfBounds,
            Self::ValueTooLarge { .. } => StatusCode::SizeLimit,
            Self::MemoryExhausted => StatusCode::StorageUnavailable,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::InvalidUtf8(_) => "Invalid UTF-8 string".to_string(),
            Self::InvalidInteger => "Value is not a valid integer".to_string(),
            Self::InvalidFloat => "Value is not a valid float".to_string(),
            Self::IndexOutOfBounds { .. } => "Index out of bounds".to_string(),
            Self::ValueTooLarge { .. } => "Value exceeds size limit".to_string(),
            Self::MemoryExhausted => "Internal server error".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        vec![
            ("error_type", "sds".to_string()),
            ("status_code", self.status_code().to_string()),
        ]
    }
}

impl From<Utf8Error> for SdsError {
    fn from(err: Utf8Error) -> Self {
        Self::InvalidUtf8(err)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Tests
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_invalid_utf8_display_and_source() {
        let bytes = vec![0xff];
        let utf8_err = std::str::from_utf8(&bytes).unwrap_err();
        let err = SdsError::InvalidUtf8(utf8_err);

        assert!(err.to_string().contains("UTF-8 decoding failed"));
        assert!(err.source().is_some());
    }

    #[test]
    fn test_invalid_integer() {
        let err = SdsError::InvalidInteger;
        assert_eq!(err.status_code(), StatusCode::InvalidInteger);
        assert_eq!(err.client_message(), "Value is not a valid integer");
        assert!(err.source().is_none());
    }

    #[test]
    fn test_invalid_float() {
        let err = SdsError::InvalidFloat;
        assert_eq!(err.status_code(), StatusCode::InvalidFloat);
        assert_eq!(err.client_message(), "Value is not a valid float");
    }

    #[test]
    fn test_index_out_of_bounds() {
        let err = SdsError::IndexOutOfBounds {
            len: 5,
            index: 10,
        };
        assert_eq!(err.status_code(), StatusCode::IndexOutOfBounds);
        assert!(err.to_string().contains("index 10"));
        assert!(err.to_string().contains("length 5"));
        assert_eq!(err.client_message(), "Index out of bounds");
    }

    #[test]
    fn test_value_too_large() {
        let err = SdsError::ValueTooLarge {
            max: 1024,
            actual: 2048,
        };
        assert_eq!(err.status_code(), StatusCode::SizeLimit);
        assert!(err.to_string().contains("2048"));
        assert!(err.to_string().contains("1024"));
    }

    #[test]
    fn test_memory_exhausted() {
        let err = SdsError::MemoryExhausted;
        assert_eq!(err.status_code(), StatusCode::StorageUnavailable);
        assert_eq!(err.client_message(), "Internal server error");
    }

    #[test]
    fn test_from_utf8_error() {
        let bytes = vec![0xff];
        let utf8_err = std::str::from_utf8(&bytes).unwrap_err();
        let sds_err: SdsError = utf8_err.into();

        matches!(sds_err, SdsError::InvalidUtf8(_));
        assert_eq!(sds_err.status_code(), StatusCode::InvalidUtf8);
    }

    #[test]
    fn test_as_any_downcast() {
        let err = SdsError::InvalidInteger;
        let any_ref = err.as_any();
        let down = any_ref.downcast_ref::<SdsError>();
        assert!(down.is_some());
    }

    #[test]
    fn test_metrics_tags() {
        let err = SdsError::InvalidFloat;
        let tags = err.metrics_tags();
        assert!(tags.iter().any(|(k, _)| k == &"error_type"));
        assert!(tags.iter().any(|(k, _)| k == &"status_code"));
    }

    #[test]
    fn test_all_variants_are_send_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<SdsError>();
    }
}
