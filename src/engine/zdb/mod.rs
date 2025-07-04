//! Модуль сериализации и десериализации значений `Value`.
//!
//! Предоставляет:
//! - [`encode`] — кодирование структуры `Value` в бинарный формат;
//! - [`decode`] — парсинг бинарного потока в `Value`;
//! - [`tags`] — список тегов для типов значений.
//! - [`compression`] — сжатие и распаковка данных для бинарного формата.
//! - [`file`] — константы магии и версии формата дампа.
//!
//! Используется в хранилище для записи и восстановления данных на диске.

pub mod compression;
pub mod decode;
pub mod encode;
pub mod file;
pub mod tags;

// Публичный экспорт всех типов ошибок и функций из вложенных модулей,
// чтобы упростить доступ к ним из внешнего кода.
pub use compression::*;
pub use decode::*;
pub use encode::*;
pub use file::*;
pub use tags::*;
