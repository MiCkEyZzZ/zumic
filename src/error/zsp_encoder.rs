use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZspEncodeError {
    #[error("Invalid data for encoding: {0}")]
    InvalidData(String),

    #[error("Invalid encoder state: {0}")]
    InvalidState(String),

    #[error("Size limit exceeded for {data_type}: {current} > {max}")]
    SizeLimit {
        current: usize,
        max: usize,
        data_type: String,
    },

    #[error("Depth limit exceeded: {current} > {max}")]
    DepthLimit { current: usize, max: usize },

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Serialization error for {type_name}: {reason}")]
    SerializationError { type_name: String, reason: String },

    #[error("String contains invalid characters (CR/LF): {0}")]
    InvalidStringFormat(String),

    #[error("I/O error during encoding: {0}")]
    IoError(#[from] std::io::Error),
}
