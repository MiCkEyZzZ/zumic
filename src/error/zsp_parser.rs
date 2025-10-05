use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZspParserError {
    #[error("Unknown command")]
    UnknownCommand,

    #[error("Wrong number of arguments for {0}: expected {1}")]
    WrongArgCount(&'static str, usize),

    #[error("Wrong number of arguments for MSET: arguments must be key-value pairs")]
    MSetWrongArgCount,

    #[error("Command must be a string")]
    CommandMustBeString,

    #[error("Expected array as command format")]
    ExpectedArray,

    #[error("Invalid key for command {0}")]
    InvalidKey(&'static str),

    #[error("Invalid UTF-8 encoding")]
    InvalidUtf8,

    #[error("Command '{0}' requires version negotiation")]
    RequiresVersionNegotiation(String),

    #[error("Unexpected handshake frame: {0}")]
    UnexpectedHandshake(String),

    #[error("Command '{0}' not implemented yet")]
    CommandNotImplemented(String),

    #[error("Extended type frames not implemented")]
    ExtendedTypeNotImplemented,
}
