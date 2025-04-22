use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Expected array for command")]
    ExpectedArray,

    #[error("Command must be a string")]
    CommandMustBeString,

    #[error("Invalid UTF-8 in command")]
    InvalidUtf8,

    #[error("Unknown command")]
    UnknownCommand,

    #[error("{0}: invalid key")]
    InvalidKey(&'static str),

    #[error("{0}: key must be valid UTF-8")]
    KeyNotUtf8(&'static str),

    #[error("{0}: unsupported value type")]
    InvalidValueType(&'static str),

    #[error("{0} requires {1} argument(s)")]
    WrongArgCount(&'static str, usize),

    #[error("MSET requires even number of arguments after command")]
    MSetWrongArgCount,
}
