pub mod ext;
pub mod macros;
pub mod stack;
pub mod status_code;
pub mod types;

// Публичный экспорт всех типов ошибок и функций из вложенных
// модулей, чтобы упростить доступ к ним из внешнего кода.
pub use ext::*;
pub use macros::*;
pub use stack::*;
pub use status_code::*;
pub use types::*;

pub type ZumicResult<T> = Result<T, StackError>;
