//! Модуль сериализации и десериализации значений `Value`.
//!
//! ## Архитектура
//!
//! Модуль предоставляет два подхода к работе с дампами:
//!
//! #### 1. Streaming API
//!
//! Event-driven парсинг для обработки больших дампов без загрузки в память:
//!
//! ```no_run
//! use std::fs::File;
//!
//! use zumic::engine::zdb::streaming::{CollectHandler, StreamingParser};
//!
//! let file = File::open("dump.zdb")?;
//! let mut parser = StreamingParser::new(file)?;
//! let mut handler = CollectHandler::new();
//! parser.parse(&mut handler)?;
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! ### 2. Legacy API (обратная совместимость)
//!
//! Традиционные функции `read_dump()` и `write_dump()`:
//!
//! ```no_run
//! use std::fs::File;
//!
//! use zumic::engine::zdb::{read_dump, write_dump};
//!
//! // Чтение
//! let mut file = File::open("dump.zdb")?;
//! let items = read_dump(&mut file)?;
//!
//! // Запись
//! let mut file = File::create("dump.zdb")?;
//! write_dump(&mut file, items.into_iter())?;
//! # Ok::<(), std::io::Error>(())
//! ```
//! ## Модули
//!
//! - [`streaming`] - SAX-style event-driven parser
//! - [`encode`] — сериализация значений в бинарный формат
//! - [`decode`] — десериализация из бинарного формата
//! - [`compression`] — сжатие и распаковка данных
//! - [`file`] — версионирование и форматы дампов
//! - [`tags`] — константы тегов для типов данных
//!
//! Используется в хранилище для записи и восстановления данных на диске.

pub mod compression;
pub mod decode;
pub mod encode;
pub mod file;
pub mod streaming;
pub mod tags;

// Публичный экспорт всех типов ошибок и функций из вложенных модулей,
// чтобы упростить доступ к ним из внешнего кода.
pub use compression::*;
pub use decode::*;
pub use encode::*;
pub use file::*;
pub use streaming::*;
pub use tags::*;
