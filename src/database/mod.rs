//! Основные структуры данных базы данных.
//!
//! В этом модуле определяются ключевые строительные блоки для реализации типов
//! данных:
//!
//! - `bitmap`: динамические битовые массивы с поддержкой операций `SETBIT`,
//!   `GETBIT`, `BITCOUNT`, `BITOP`.
//! - `dict`: реализация словаря на основе хеш-таблицы.
//! - `geo`: географические множества и операции (GEOADD, GEORADIUS, GEODIST и
//!   т.п.).
//! - `int_set`: компактное множество целых чисел для небольших коллекций.
//! - `list_pack`: компактная структура списка для эффективного хранения.
//! - `lua`: привязки и контекст для встроенного Lua.
//! - `quicklist`: гибридный список, сочетающий связные списки и зиплисты.
//! - `sds`: простые динамические строки (SDS), похожие на внутренние строки
//!   Redis.
//! - `skip_list`: скип-лист для быстрого доступа к отсортированным данным.
//! - `smart_hash`: автоматически масштабируемая хеш-таблица с оптимизациями.
//! - `stream`: структуры для работы с потоками данных.
//! - `types`: определяет корневые типы `Value`, хранящиеся в базе.
//! - `pubsub_manager`: менеджер Pub/Sub — публикация, подписка и статистика.
//!
//! Публичный экспорт всех подмодулей и их функций упрощает доступ из внешнего
//! кода.

pub mod bitmap;
pub mod dict;
pub mod expire;
pub mod geo;
pub mod hll;
pub mod intset;
pub mod listpack;
pub mod lua;
pub mod pubsub_manager;
pub mod quicklist;
pub mod sds;
pub mod skiplist;
pub mod smarthash;
pub mod stream;
pub mod types;

// Публичный экспорт всех типов ошибок и функций из вложенных
// модулей, чтобы упростить доступ к ним из внешнего кода.
pub use bitmap::*;
pub use dict::*;
pub use expire::*;
pub use geo::*;
pub use hll::*;
pub use intset::*;
pub use listpack::*;
pub use pubsub_manager::*;
pub use quicklist::*;
pub use sds::*;
pub use skiplist::*;
pub use smarthash::*;
pub use stream::*;
pub use types::*;
