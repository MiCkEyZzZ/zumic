pub mod ext;
pub mod macros;
pub mod stack;
pub mod status_code;
pub mod types;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use ext::*;
pub use macros::*;
pub use stack::*;
pub use status_code::*;
pub use types::*;

pub type ZumicResult<T> = Result<T, StackError>;
