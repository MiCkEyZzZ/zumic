pub mod ext;
pub mod macros;
pub mod stack;
pub mod status_code;
pub mod types;

pub use ext::*;
pub use macros::*;
pub use stack::*;
pub use status_code::*;
pub use types::*;

pub type ZumicResult<T> = Result<T, StackError>;
