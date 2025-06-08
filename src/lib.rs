//! Zumic - a high-performance in-memory key-value engine.
//!
//! Main modules:
//! - `auth` — authentication and ACL (users, passwords, access control rules)
//! - `command` — command parsing and execution (SET, GET, INCR, etc.)
//! - `config` — server configuration loading
//! - `database` — built-in data structures (Dict, SkipList, QuickList, SDS, etc.)
//! - `engine` — storage engine abstractions and implementations (InMemory, Persistent, Cluster)
//! - `error` — common error types (encoding/decoding, parsing, storage)
//! - `logging` — flexible logging (formatting, filters, sinks)
//! - `network` — networking: ZSP protocol and Tokio server
//! - `pubsub` — Pub/Sub: broker, subscriptions, messages

/// Authentication and ACL: users, passwords, access control rules.
pub mod auth;
/// Authentication and ACL: users, passwords, access control rules.
pub mod command;
/// Server configuration loading.
pub mod config;
/// Built-in data structures (Dict, SkipList, QuickList, SDS).
pub mod database;
/// Storage engine abstractions and implementations (InMemory, Persistent, Cluster).
pub mod engine;
/// Common error types: encoding/decoding, parsing, storage.
pub mod error;
/// Flexible logging (formatting, filters, sinks).
pub mod logging;
/// Networking: ZSP protocol and Tokio-based server.
pub mod network;
/// Pub/Sub: broker, subscriptions, messages.
pub mod pubsub;

// -----------------------------------------------------------------------------
//  Frequently used public types
// -----------------------------------------------------------------------------

/// Hashing and password verification functions, ACL manager.
pub use auth::{
    hash_password, verify_password, Acl, AclRule, AclUser, AuthManager, CmdCategory, ServerConfig,
    UserConfig,
};

/// Core key-value commands: SET, GET, INCR, HSET, LPOP, ZADD, etc.
pub use command::{
    AppendCommand, AuthCommand, Command, CommandExecute, DecrByCommand, DecrByFloatCommand,
    DecrCommand, DelCommand, ExistsCommand, FlushDbCommand, GetCommand, GetRangeCommand,
    HDelCommand, HGetAllCommand, HGetCommand, HSetCommand, IncrByCommand, IncrByFloatCommand,
    IncrCommand, LLenCommand, LPopCommand, LPushCommand, LRangeCommand, MGetCommand, MSetCommand,
    RPopCommand, RPushCommand, RenameCommand, RenameNxCommand, SAddCommand, SCardCommand,
    SIsMemberCommand, SMembersCommand, SRemCommand, SetCommand, SetFloatCommand, SetNxCommand,
    StrLenCommand, ZAddCommand, ZCardCommand, ZRangeCommand, ZRemCommand, ZRevRangeCommand,
    ZScoreCommand,
};

/// Configuration types.
pub use config::{Settings, StorageConfig, StorageType};

/// Data structures: Dict, QuickList, SkipList, Sds, and others.
pub use database::{
    Dict, GeoEntry, GeoPoint, GeoSet, Hll, ListPack, QuickList, Sds, SkipList, SmartHash,
    StreamEntry, Value,
};

/// Storage engines: InMemoryStore, InPersistentStore, InClusterStore.
pub use engine::{
    load_from_zdb, save_to_zdb, AofLog, InClusterStore, InMemoryStore, InPersistentStore, Storage,
    StorageEngine,
};

/// Operation errors and result types.
pub use error::{
    AclError, AuthError, ConfigError, DecodeError, EncodeError, NetworkError, ParseError,
    PasswordError, StoreError, StoreResult,
};

/// Network server and protocol.
pub use network::{server, zsp};

/// Pub/Sub API.
pub use pubsub::{Broker, Message, PatternSubscription, Subscription};
