/// Authentication and ACL: users, passwords, access control rules.
pub mod auth;
/// Command-line parsing and execution (SET, GET, etc.).
pub mod command;
/// Server configuration loading.
pub mod config;
/// Built-in data structures (Dict, SkipList, QuickList, SDS).
pub mod database;
/// Storage engine abstraction and implementations (InMemory, Persistent, Cluster).
pub mod engine;
/// Common error types: encoding/decoding, parsing, storage.
pub mod error;
/// Flexible logging (formatting, filters, sinks).
pub mod logging;
/// Network stack: ZSP protocol and Tokio-based server.
pub mod network;
/// Pub/Sub: Broker, Subscription, Message.
pub mod pubsub;

// -----------------------------------------------------------------------------
//  Frequently used public types
// -----------------------------------------------------------------------------

/// Password hashing and verification functions, ACL manager.
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
/// config
pub use config::{Settings, StorageConfig, StorageType};
/// Data types: Dict, QuickList, SkipList, Sds, and others.
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
