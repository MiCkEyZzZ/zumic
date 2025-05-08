//! Core database data structures.
//!
//! This module defines the fundamental building blocks used to implement
//! Redis-like data types:
//!
//! - `dict`: generic dictionary (hash map) implementation.
//! - `int_set`: compact integer set for storing small integer collections.
//! - `list_pack`: compact list structure for memory-efficient storage.
//! - `lua`: bindings or context for embedded Lua scripting.
//! - `quicklist`: hybrid list combining linked lists and ziplists.
//! - `sds`: simple dynamic strings (SDS), similar to Redis internal strings.
//! - `skip_list`: skip list for sorted data with fast access.
//! - `smart_hash`: auto-scaling hash table with support for small optimizations.
//! - `types`: defines `Value` types stored in the database.

pub mod dict;
pub mod geo;
pub mod hll;
pub mod int_set;
pub mod list_pack;
pub mod lua;
pub mod quicklist;
pub mod sds;
pub mod skip_list;
pub mod smart_hash;
pub mod stream;
pub mod types;

pub use dict::*;
pub use geo::*;
pub use hll::*;
pub use list_pack::*;
pub use quicklist::*;
pub use sds::*;
pub use skip_list::*;
pub use smart_hash::*;
pub use stream::*;
pub use types::*;
