//! Определения команд и логика их выполнения.
//!
//! Этот модуль содержит реализацию поддерживаемых команд базы
//! данных, структурированную по функциональным группам:
//!
//! - `auth`: команды, связанные с аутентификацией.
//! - `basic`: основные утилитарные команды (например, `ping`, `echo`, `select`).
//! - `bitmap`: команды для работы с битовыми массивами (`SETBIT`, `GETBIT`, `BITCOUNT`, `BITOP`).
//! - `execute`: диспетчер команд и их регистрация.
//! - `float`: команды для операций с числами с плавающей точкой.
//! - `hash`: команды для работы со структурами, подобными хешам.
//! - `int`: команды для целочисленных операций и счётчиков.
//! - `list`: команды для списков (push, pop, range и т. д.).
//! - `set`: команды для работы с неупорядоченными множествами.
//! - `string`: команды для работы со строками и их хранения.
//! - `zset`: команды для работы с отсортированными множествами.

pub mod auth;
pub mod basic;
pub mod bitmap;
pub mod execute;
pub mod float;
pub mod geo;
pub mod hash;
pub mod int;
pub mod list;
pub mod set;
pub mod string;
pub mod zset;

// Публичный экспорт всех типов ошибок и функций из вложенных
// модулей, чтобы упростить доступ к ним из внешнего кода.
pub use auth::*;
pub use basic::*;
pub use bitmap::*;
pub use execute::*;
pub use float::*;
pub use geo::*;
pub use hash::*;
pub use int::*;
pub use list::*;
pub use set::*;
pub use string::*;
pub use zset::*;
