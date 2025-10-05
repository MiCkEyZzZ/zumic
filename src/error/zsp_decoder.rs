use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZspDecodeError {
    #[error("Invalid decoder state: {0}")]
    InvalidState(String),

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

    #[error("Size limit exceeded for {data_type}: {current} > {max}")]
    SizeLimit {
        current: usize,
        max: usize,
        data_type: String,
    },

    #[error("Data corruption at position {position}: expected {expected}, found {found}")]
    Corruption {
        position: usize,
        expected: String,
        found: String,
    },

    #[error("Invalid float format: {0}")]
    InvalidFloat(String),

    #[error("Invalid boolean format: {0}")]
    InvalidBoolean(String),

    #[error("Depth limit exceeded: {current} > {max}")]
    DepthLimit { current: usize, max: usize },
}
