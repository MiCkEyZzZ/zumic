pub mod decode;
pub mod encode;
pub mod global;
pub mod parser;

pub use decode::DecodeError;
pub use encode::EncodeError;
pub use global::{StoreError, StoreResult};
pub use parser::ParseError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("Decode error: {0}")]
    Decode(#[from] DecodeError),

    #[error("Encode error: {0}")]
    Encode(#[from] EncodeError),
}
