use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZspSerializationError {
    #[error("Serialization requires version negotiation")]
    RequiresVersionNegotiation,

    #[error("JSON serialization error: {0}")]
    JsonError(String),

    #[error("Data conversion error: {0}")]
    ConversionError(String),

    #[error("Invalid data type for serialization: {0}")]
    InvalidDataType(String),
}
