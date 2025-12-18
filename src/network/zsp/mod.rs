//! Модуль `zsp` реализует сетевой протокол ZSP (Zumic Serialization Protocol).
//!
//! Он включает в себя:
//! - Субмодуль `frame` – низкоуровневый разбор и формирование **фреймов**
//!   протокола (кодирование/декодирование сообщений по сети).
//! - Субмодуль `protocol` – разбор **команд** и формирование **ответов** (более
//!   высокоуровневая логика протокола).
//!
//! Из этого модуля переэкспортируются ключевые типы протокола:
//! - `ZspDecoder`, `ZspEncoder`, `ZspFrame` и константы `MAX_ARRAY_DEPTH`,
//!   `MAX_BINARY_LENGTH`, `MAX_LINE_LENGTH` для работы с фреймами.
//! - `Command` и `Response` – для представления команд клиента и ответов
//!   сервера.

pub mod frame;
pub mod protocol;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use frame::*;
pub use protocol::*;
