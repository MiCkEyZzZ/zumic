pub mod auth;
pub mod decode;
pub mod encode;
pub mod parser;
pub mod system;

pub use auth::{AclError, AuthError, ConfigError, PasswordError};
pub use decode::DecodeError;
pub use encode::EncodeError;
pub use parser::ParseError;
pub use system::{StoreError, StoreResult};

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
