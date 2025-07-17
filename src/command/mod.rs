//! Реализация всех поддерживаемых команд Zumic и их диспетчеризация.
//!
//! Этот модуль объединяет команды для различных типов данных и операций,
//! сгруппированные по функциональным подмодулям:
//!
//! - [`auth`] — аутентификация и управление доступом.
//! - [`basic`] — базовые утилитарные команды (например, `ping`, `echo`, `select`).
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
//! Все команды реализуют трейт [`CommandExecute`] и могут быть вызваны через единый интерфейс.
//!
//! # Публичные реэкспорты
//! Все основные команды доступны напрямую из этого модуля для удобства импорта:
//! ```rust
//! use zumic::command::{SetCommand, GetCommand, LPushCommand};
//! ```
//!
//! # Пример выполнения команды
//! ```rust
//! use zumic::command::{SetCommand, CommandExecute};
//! let mut store = /* ваш StorageEngine */;
//! let cmd = SetCommand { key: "foo".into(), value: /* ... */ };
//! let result = cmd.execute(&mut store);
//! ```

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
