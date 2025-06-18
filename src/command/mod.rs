//! Command definitions and execution logic.
//!
//! This module contains the implementation of supported database commands,
//! organized by functionality:
//!
//! - `auth`: authentication-related commands.
//! - `basic`: general utility commands (e.g., `ping`, `echo`, `select`).
//! - `execute`: command dispatcher and command registration.
//! - `float`: commands for floating-point operations.
//! - `hash`: commands for manipulating hash-like structures.
//! - `int`: commands for integer and counter operations.
//! - `list`: list-related commands (push, pop, range, etc.).
//! - `set`: commands for working with unordered sets.
//! - `string`: commands for string manipulation and storage.
//! - `zset`: commands for working with sorted sets.

pub mod auth;
pub mod basic;
pub mod execute;
pub mod float;
pub mod hash;
pub mod int;
pub mod list;
pub mod set;
pub mod string;
pub mod zset;

// Публичный экспорт всех типов ошибок и функций из вложенных модулей,
// чтобы упростить доступ к ним из внешнего кода.
pub use auth::*;
pub use basic::*;
pub use execute::*;
pub use float::*;
pub use hash::*;
pub use int::*;
pub use list::*;
pub use set::*;
pub use string::*;
pub use zset::*;
