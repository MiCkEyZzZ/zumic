//! Модуль `zsp` реализует сетевой протокол ZSP (Zumic Socket Protocol).
//!
//! Он включает в себя:
//! - Субмодуль `frame` – низкоуровневый разбор и формирование **фреймов**
//! протокола (кодирование/декодирование сообщений по сети).
//! - Субмодуль `protocol` – разбор **команд** и формирование **ответов**
//! (более высокоуровневая логика протокола).
//!
//! Из этого модуля переэкспортируются ключевые типы протокола:
//! - `ZspDecoder`, `ZspEncoder`, `ZspFrame` и константы `MAX_ARRAY_DEPTH`,
//! `MAX_BINARY_LENGTH`, `MAX_LINE_LENGTH` для работы с фреймами.
//! - `Command` и `Response` – для представления команд клиента и ответов сервера.

pub mod frame;
pub mod protocol;

pub use frame::{
    ZspDecodeState, ZspDecoder, ZspEncoder, ZspFrame, MAX_ARRAY_DEPTH, MAX_BINARY_LENGTH,
    MAX_LINE_LENGTH,
};
pub use protocol::{Command, Response};
