//! Модуль, отвечающий за обработку ошибок, возникающих в различных
//! частях системы.
//! Включает типы ошибок и связанные с ними утилиты.

/// Модуль аутентификации — ошибки, связанные с авторизацией и контролем
/// доступа.
pub mod auth;
pub mod client;
/// Модуль сетевого взаимодействия — ошибки, возникающие при работе с сетью.
pub mod network;
/// Модуль парсинга — ошибки при разборе входящих данных или команд.
pub mod parser;
pub mod pubsub;
pub mod slot_manager;
/// Модуль системных ошибок — общие системные ошибки, не попадающие в другие
/// категории.
pub mod system;
pub mod zdb_version;
pub mod zsp_decoder;
pub mod zsp_encoder;
pub mod zsp_parser;
pub mod zsp_serialization;

// Публичный экспорт всех типов ошибок и функций из вложенных модулей,
// чтобы упростить доступ к ним из внешнего кода.
pub use auth::*;
pub use client::*;
pub use network::*;
pub use parser::*;
pub use pubsub::*;
pub use slot_manager::*;
pub use system::*;
pub use zdb_version::*;
pub use zsp_decoder::*;
pub use zsp_encoder::*;
pub use zsp_parser::*;
pub use zsp_serialization::*;
