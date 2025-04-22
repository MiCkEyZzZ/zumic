pub mod global;
pub mod parser;

pub use global::{StoreError, StoreResult};
pub use parser::ParseError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
}
