//! Сетевой модуль Zumic.
//!
//! Включает реализацию серверной части и протокола ZSP.
//!
//! ## Подмодули
//!
//! - `banner`: реализация банера для консольного вывода информации при старте
//!   сервера Зумик
//! - `server`: реализация сетевого сервера (приём и обработка соединений).
//! - `connection`: управление соединениями и вспомогательные структуры для
//!   TCP-клиентов/серверов.
//! - `connection_state` — определения состояний соединений и связанные с ними
//!   перечисления.
//! - `zsp`: реализация собственного протокола ZSP: фрейминг, парсинг,
//!   сериализация.
//!
//! Импортируя `network`, вы получаете полный набор для работы с сетью.

pub mod admin_commands;
pub mod banner;
pub mod connection;
pub mod connection_registry;
pub mod connection_state;
pub mod server;
pub mod zsp;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use zsp::*;
