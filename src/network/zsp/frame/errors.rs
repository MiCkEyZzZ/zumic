use thiserror::Error;

/// Errors that occur when working with the ZSP protocol.
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
    #[error("UTF8 error: {0}")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}
