use thiserror::Error;

/// Ошибки, возникающие при работе с протоколом ZSP.
#[derive(Debug, Error)]
pub enum ZSPError {
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Unexpected EOF: {0}")]
    UnexpectedEof(String),
    #[error("UTF8 error: {0}")]
    Utf8Error(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    // Если хочешь, можно добавить ещё вариант для FromUtf8Error:
    #[error("UTF8 error: {0}")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}
