//! Storage engine modules.
//!
//! This module defines various implementations and abstractions for key-value storage:
//!
//! - `memory`: in-memory store for fast, ephemeral data access.
//! - `persistent`: persistent store for durable storage (e.g., on disk).
//! - `cluster`: distributed key-value store for clustered setups.
//! - `engine`: facade for selecting and interacting with a specific storage backend.
//! - `storage`: trait that defines a unified interface for all storage implementations.

pub mod aof;
pub mod cluster;
pub mod engine;
pub mod memory;
pub mod persistent;
pub mod storage;

pub use aof::AofLog;
pub use cluster::InClusterStore;
pub use engine::StorageEngine;
pub use memory::InMemoryStore;
pub use persistent::InPersistentStore;
pub use storage::Storage;
