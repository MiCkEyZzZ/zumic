//! Реализация всех поддерживаемых команд Zumic и их диспетчеризация.
//!
//! Этот модуль объединяет команды для различных типов данных и операций,
//! сгруппированные по функциональным подмодулям:
//!
//! - [`auth`] — аутентификация и управление доступом.
//! - [`keys`] — базовые утилитарные команды (например, `ping`, `echo`,
//!   `select`).
//! - [`bitmap`] — битовые операции (`SETBIT`, `GETBIT`, `BITCOUNT`, `BITOP`).
//! - [`execute`] — диспетчеризация и единый интерфейс выполнения команд.
//! - [`float`] — операции с числами с плавающей точкой.
//! - [`geo`] — географические структуры и команды.
//! - [`hash`] — ассоциативные массивы (hash).
//! - [`int`] — целочисленные операции и счётчики.
//! - [`list`] — списки (push, pop, range и т. д.).
//! - [`set`] — неупорядоченные множества.
//! - [`string`] — строки и операции над ними.
//! - [`zset`] — отсортированные множества (sorted set).
//!
//! Все команды реализуют трейт [`CommandExecute`] и могут быть вызваны через
//! единый интерфейс.

pub mod auth;
pub mod bitmap;
pub mod execute;
pub mod float;
pub mod geo;
pub mod hash;
pub mod int;
pub mod keys;
pub mod list;
pub mod pubsub;
pub mod server;
pub mod set;
pub mod string;
pub mod zset;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use auth::*;
pub use bitmap::*;
pub use execute::*;
pub use float::*;
pub use geo::*;
pub use hash::*;
pub use int::*;
pub use keys::*;
pub use list::*;
pub use server::*;
pub use set::*;
pub use string::*;
pub use zset::*;
