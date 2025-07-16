//! Zumic — высокопроизводительный движок хранения ключ-значение в памяти.
//!
//! Основные модули:
//! - `auth` — аутентификация и контроль доступа (пользователи, пароли, правила доступа)
//! - `command` — разбор и выполнение команд (SET, GET, INCR и др.)
//! - `config` — загрузка конфигурации сервера
//! - `database` — встроенные структуры данных (Dict, SkipList, QuickList, SDS и др.)
//! - `engine` — абстракции и реализации движков хранения (InMemory, Persistent, Cluster)
//! - `error` — типы ошибок (кодирование/декодирование, парсинг, хранение)
//! - `logging` — гибкая система логирования (форматирование, фильтры, вывод)
//! - `network` — работа с сетью: протокол ZSP и сервер на базе Tokio
//! - `pubsub` — Pub/Sub: брокер, подписки, сообщения

/// Аутентификация и контроль доступа: пользователи, пароли, правила доступа.
pub mod auth;
/// Разбор и выполнение команд: SET, GET, INCR и др.
pub mod command;
pub mod command_registry;
/// Загрузка конфигурации сервера.
pub mod config;
/// Встроенные структуры данных (Dict, SkipList, QuickList, SDS).
pub mod database;
pub mod db_context;
/// Абстракции и реализации движков хранения (InMemory, Persistent, Cluster).
pub mod engine;
/// Типы ошибок: кодирование/декодирование, парсинг, хранение.
pub mod error;
/// Гибкая система логирования (форматирование, фильтры, вывод).
pub mod logging;
pub mod modules;
/// Работа с сетью: протокол ZSP и сервер на Tokio.
pub mod network;
/// Pub/Sub: брокер, подписки, сообщения.
pub mod pubsub;

// -----------------------------------------------------------------------------
//  Часто используемые публичные типы
// -----------------------------------------------------------------------------

/// Функции хеширования и проверки пароля, менеджер ACL.
pub use auth::{
    hash_password, verify_password, Acl, AclRule, AclUser, AuthManager, CmdCategory, ServerConfig,
    UserConfig,
};

/// Основные команды key-value: SET, GET, INCR, HSET, LPOP, ZADD и др.
pub use command::{
    AppendCommand, AuthCommand, Command as StoreCommand, CommandExecute, DecrByCommand,
    DecrByFloatCommand, DecrCommand, DelCommand, ExistsCommand, FlushDbCommand, GetCommand,
    GetRangeCommand, HDelCommand, HGetAllCommand, HGetCommand, HSetCommand, IncrByCommand,
    IncrByFloatCommand, IncrCommand, LLenCommand, LPopCommand, LPushCommand, LRangeCommand,
    MGetCommand, MSetCommand, RPopCommand, RPushCommand, RenameCommand, RenameNxCommand,
    SAddCommand, SCardCommand, SIsMemberCommand, SMembersCommand, SRemCommand, SetCommand,
    SetFloatCommand, SetNxCommand, StrLenCommand, ZAddCommand, ZCardCommand, ZRangeCommand,
    ZRemCommand, ZRevRangeCommand, ZScoreCommand,
};

/// Структуры данных: Dict, QuickList, SkipList, Sds и другие.
pub use database::{
    Dict, GeoEntry, GeoPoint, GeoSet, Hll, ListPack, QuickList, Sds, SkipList, SmartHash,
    StreamEntry, Value, DENSE_SIZE,
};

/// Движки хранения: InMemoryStore, InPersistentStore, InClusterStore.
pub use engine::{
    load_from_zdb, save_to_zdb, AofLog, InClusterStore, InMemoryStore, InPersistentStore, Storage,
    StorageEngine,
};

/// Сетевой сервер и протокол.
pub use network::{server, zsp};

/// API для Pub/Sub.
pub use pubsub::{Broker, Message, PatternSubscription, Subscription};

pub use error::{
    AclError, AuthError, ConfigError, DecodeError, EncodeError, NetworkError, ParseError,
    PasswordError, StoreError, StoreResult, VersionError,
};

pub use config::settings::{Settings, StorageConfig, StorageType};

pub use modules::{DynamicModule, Manager, Module, Plugin, WasmPlugin};
