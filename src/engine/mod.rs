//! Database primitives and data structures.
//!
//! This module provides core data types and abstractions for
//! implementing an in-memory database:
//!
//! - `dict`: generic key-value hash map with support for custom key types.
//! - `int_set`: compact set for storing integers efficiently.
//! - `list_pack`: compact serialization format for storing small lists of elements.
//! - `lua`: support for running embedded Lua scripts.
//! - `quicklist`: memory-efficient doubly linked list supporting compression.
//! - `sds`: simple dynamic string type optimized for performance.
//! - `skip_list`: sorted data structure with logarithmic operations.
//! - `smart_hash`: hybrid hash table optimized for memory and speed.
//! - `types`: definitions for supported value types in the database.

pub mod aof;
pub mod cluster;
pub mod engine;
pub mod memory;
pub mod persistent;
pub mod storage;
pub mod zdb;
pub mod zdb_protocol;

pub use aof::*;
pub use cluster::*;
pub use engine::*;
pub use memory::*;
pub use persistent::*;
pub use storage::*;
pub use zdb::*;
pub use zdb_protocol::*;
