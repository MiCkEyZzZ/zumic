pub mod api;
pub mod loader;
pub mod plugin_manager;
pub mod wasm;

// Публичный экспорт всех типов ошибок и функций из вложенных модулей,
// чтобы упростить доступ к ним из внешнего кода.
pub use api::*;
pub use loader::*;
pub use plugin_manager::*;
pub use wasm::*;
