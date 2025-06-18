use thiserror::Error;

use super::{decode::DecodeError, encode::EncodeError, parser::ParseError};

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("Decode error: {0}")]
    Decode(#[from] DecodeError),

    #[error("Encode error: {0}")]
    Encode(#[from] EncodeError),
}
