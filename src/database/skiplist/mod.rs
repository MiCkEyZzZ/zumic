//! SkipList - потокобезопасная реализация для Zumic.
//!
//! # Модули
//!
//! - `skiplist_base`: базовая однопоточная реализация.
//! - `concurrent`: потокобезопасная обёртка с `Arc<RwLock>`
//! - `sharded`: сегментированная поставка для высокого параллелизма.
//! - `безопасность`: валидация и статистика

pub mod safety;
pub mod skiplist_base;

#[cfg(feature = "concurrent")]
pub mod concurrent;
#[cfg(feature = "concurrent")]
pub mod sharded;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
#[cfg(feature = "concurrent")]
pub use concurrent::*;
pub use safety::*;
#[cfg(feature = "concurrent")]
pub use sharded::*;
pub use skiplist_base::*;
