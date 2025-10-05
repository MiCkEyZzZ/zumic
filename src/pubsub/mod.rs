//! Подсистема Publish–Subscribe (pub/sub).
//!
//! Этот модуль реализует лёгкую систему pub/sub для внутрипроцессного вещания
//! сообщений и управления подписками:
//!
//! - `broker`: управление регистрацией тем, подписками и доставкой сообщений.
//! - `intern` (приватный): внутренние утилиты каналов для координации
//!   подписчиков.
//! - `message`: структура сообщений и метаданные для публикуемых событий.
//! - `subscriber`: логика подписок и интерфейсы потоков для потребителей.
//! - `zsp_integration`: интеграция с ZSP-протоколом для pub/sub.
//!
//! Публичный API переэкспортирует:
//! - `broker::*`
//! - `message::*`
//! - `subscriber::*`
//! - `zsp_integration::*`

pub mod broker;
mod intern;
pub mod message;
pub mod subscriber;
pub mod zsp_integration;

// Публичный экспорт всех типов ошибок и функций из вложенных
// модулей, чтобы упростить доступ к ним из внешнего кода.
pub use broker::*;
pub(crate) use intern::intern_channel;
pub use message::*;
pub use subscriber::*;
pub use zsp_integration::*;
