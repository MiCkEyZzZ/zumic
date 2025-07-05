pub mod config;
pub mod filters;
pub mod formatter;
pub mod sinks;

// Публичный экспорт всех типов ошибок и функций из вложенных модулей,
// чтобы упростить доступ к ним из внешнего кода.
pub use config::init_logging;
