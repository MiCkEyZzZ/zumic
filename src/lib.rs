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
/// Реестр всех поддерживаемых команд и их обработчиков.
pub mod command_registry;
/// Загрузка конфигурации сервера.
pub mod config;
/// Встроенные структуры данных (Dict, SkipList, QuickList, SDS).
pub mod database;
/// Контекст базы данных: управление состоянием, транзакциями и доступом к данным.
pub mod db_context;
/// Абстракции и реализации движков хранения (InMemory, Persistent, Cluster).
pub mod engine;
/// Типы ошибок: кодирование/декодирование, парсинг, хранение.
pub mod error;
/// Гибкая система логирования (форматирование, фильтры, вывод).
pub mod logging;
/// Модули расширения: API для загрузки, управления и интеграции плагинов.
pub mod modules;
/// Работа с сетью: протокол ZSP и сервер на Tokio.
pub mod network;
/// Pub/Sub: брокер, подписки, сообщения.
pub mod pubsub;

// -----------------------------------------------------------------------------
//  Часто используемые публичные типы
// -----------------------------------------------------------------------------

/// Реэкспорт основных структур и функций для работы с ACL и аутентификацией.
pub use auth::{
    hash_password, verify_password, Acl, AclRule, AclUser, AuthManager, CmdCategory, ServerConfig,
    UserConfig,
};

/// Реэкспорт основных команд key-value.
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

/// Реэкспорт встроенных структур данных.
pub use database::{
    Bitmap, Dict, DictIter, ExpireMap, GeoEntry, GeoPoint, GeoSet, Hll, IntSet, ListPack, Node,
    PubSubManager, QuickList, RangeIter, ReverseIter, Sds, SkipList, SkipListIter, SmartHash,
    SmartHashIter, Stream, StreamEntry, StreamId, Value, DENSE_SIZE,
};

/// Реэкспорт движков хранения.
pub use engine::{
    load_from_zdb, save_to_zdb, AofLog, InClusterStore, InMemoryStore, InPersistentStore, Storage,
    StorageEngine,
};

/// Реэкспорт сетевого сервера и протокола.
pub use network::{banner, server, zsp};

/// Реэкспорт API для Pub/Sub.
pub use pubsub::{
    Broker, BrokerConfig, BrokerMetrics, BrokerSnapshot, ChannelSnapshot, ChannelStats,
    ContentFilter, LagHandling, Message, MessageFilters, MessageMetadata, MessagePayload,
    MessageResult, MessageTypeFilter, MetadataFilter, MultiSubscriber, PayloadType, PublishOptions,
    PublishResult, SerializationFormat, SizeFilter, Subscriber, SubscriberStats,
    SubscriptionOptions,
};

/// Реэкспорт основных типов ошибок.
pub use error::{
    AclError, AuthError, ConfigError, DecodeError, EncodeError, NetworkError, ParseError,
    PasswordError, RecvError, StoreError, StoreResult, TryRecvError, VersionError,
};

/// Реэкспорт настроек конфигурации.
pub use config::settings::{Settings, StorageConfig, StorageType};

/// Реэкспорт API для работы с модулями и плагинами.
pub use modules::{DynamicModule, Manager, Module, Plugin, WasmPlugin};
