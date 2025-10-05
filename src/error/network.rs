use thiserror::Error;

use crate::{ParseError, ZspDecodeError, ZspEncodeError};

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("Decode error: {0}")]
    Decode(#[from] ZspDecodeError),

    #[error("Encode error: {0}")]
    Encode(#[from] ZspEncodeError),
}
