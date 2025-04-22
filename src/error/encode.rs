use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Unexpected EOF: {0}")]
    UnexpectedEof(String),

    #[error("Invalid UTF-8 encoding: {0}")]
    InvalidUtf8(String),

    #[error("Invalid integer: {0}")]
    InvalidInteger(String),

    #[error("Maximum array depth exceeded: {0}")]
    MaxArrayDepthExceeded(String),

    #[error("Invalid frame type: {0}")]
    InvalidFrameType(String),
}
