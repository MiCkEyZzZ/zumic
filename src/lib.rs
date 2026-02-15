//! Zumic — высокопроизводительный движок хранения ключ-значение в памяти.
//!
//! Основные модули:
//! - `auth` — аутентификация и контроль доступа (пользователи, пароли, правила
//!   доступа)
//! - `command` — разбор и выполнение команд (SET, GET, INCR и др.)
//! - `config` — загрузка конфигурации сервера
//! - `database` — встроенные структуры данных (Dict, SkipList, QuickList, SDS и
//!   др.)
//! - `engine` — абстракции и реализации движков хранения (InMemory, Persistent,
//!   Cluster)
//! - `error` — типы ошибок (кодирование/декодирование, парсинг, хранение)
//! - `logging` — гибкая система логирования (форматирование, фильтры, вывод)
//! - `network` — работа с сетью: протокол ZSP и сервер на базе Tokio
//! - `pubsub` — Pub/Sub: брокер, подписки, сообщения

/// Аутентификация и контроль доступа: пользователи, пароли, правила доступа.
pub mod auth;
pub mod client;
/// Разбор и выполнение команд: SET, GET, INCR и др.
pub mod command;
/// Реестр всех поддерживаемых команд и их обработчиков.
pub mod command_registry;
/// Загрузка конфигурации сервера.
pub mod config;
/// Встроенные структуры данных (Dict, SkipList, QuickList, SDS).
pub mod database;
/// Контекст базы данных: управление состоянием, транзакциями и доступом к
/// данным.
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
    AclDelUserCommand, AclGetUserCommand, AclSetUserCommand, AppendCommand, AuthCommand,
    BgSaveCommand, BitCountCommand, BitOpCommand, BitPosCommand, Command as StoreCommand,
    CommandExecute, CommandExecutor, DbSizeCommand, DecrByCommand, DecrByFloatCommand, DecrCommand,
    DelCommand, EchoCommand, ExistsCommand, FlushDbCommand, GeoAddCommand, GeoPosCommand,
    GeoRadiusByMemberCommand, GeoRadiusCommand, GetBitCommand, GetCommand, GetDistCommand,
    GetRangeCommand, HDelCommand, HExistsCommand, HGetAllCommand, HGetCommand, HIncrByCommand,
    HIncrByFloatCommand, HKeysCommand, HLenCommand, HRandFieldCommand, HSetCommand, HValsCommand,
    HmGetCommand, IncrByCommand, IncrByFloatCommand, IncrCommand, InfoCommand, LLenCommand,
    LPopCommand, LPushCommand, LRangeCommand, LRemCommand, LSetCommand, MGetCommand, MSetCommand,
    PfAddCommand, PfCountCommand, PfMergeCommand, PingCommand, RPopCommand, RPushCommand,
    RenameCommand, RenameNxCommand, SAddCommand, SCardCommand, SDiffCommand, SInterCommand,
    SIsMemberCommand, SMembersCommand, SPopCommand, SRandMemberCommand, SRemCommand, SUnionCommand,
    SaveCommand, SelectCommand, SetBitCommand, SetCommand, SetFloatCommand, SetNxCommand,
    ShutdownCommand, StrLenCommand, TimeCommand, XAckCommand, XAddCommand, XDelCommand,
    XGroupCreateCommand, XLenCommand, XRangeCommand, XReadCommand, XRevRangeCommand, XTrimCommand,
    ZAddCommand, ZCardCommand, ZCountCommand, ZIncrByCommand, ZRangeCommand, ZRankCommand,
    ZRemCommand, ZRevRangeCommand, ZRevRankCommand, ZScoreCommand,
};
/// Реэкспорт настроек конфигурации.
pub use config::settings::{Settings, StorageConfig, StorageType};
/// Реэкспорт встроенных структур данных.
pub use database::{
    haversine_distance, Bitmap, BoundingBox, ConcurrentSkipList, ContentionMetrics,
    ContentionSnapshot, Dict, DictIter, Direction, ExpireMap, FragmentationInfo, GeoEntry,
    GeoModuleStats, GeoPoint, GeoSet, Geohash, GeohashPrecision, GeohashStats, HashMetrics, Hll,
    HllBuilder, HllCompact, HllDefault, HllDense, HllEncoding, HllHasher, HllMaxPrecision,
    HllPrecise, HllSparse, HllStats, IntSet, IntSetIter, IntSetRangeIter, ListPack, MurmurHasher,
    Node, QuickList, RTree, RadiusOptions, RangeIter, ReverseIter, Sds, SipHasher, SkipList,
    SkipListIter, SkipListStatistics, SmartHash, SmartHashIter, Stream, StreamEntry, StreamId,
    TreeStats, ValidationError, Value, XxHasher, BIT_COUNT_TABLE, DEFAULT_PRECISION,
    DEFAULT_SPARSE_THRESHOLD, GEO_VERSION, MAX_PRECISION, MIN_PRECISION, SERIALIZATION_VERSION,
};
/// Реэкспорт движков хранения.
pub use engine::{
    load_from_zdb, save_to_zdb, AofLog, GlobalShardStats, InMemoryStore, InPersistentStore, Shard,
    ShardId, ShardMetrics, ShardMetricsSnapshot, ShardedIndex, ShardingConfig, SlotId, SlotManager,
    SlotState, Storage, StorageEngine, SyncPolicy,
};
/// Реэкспорт основных типов ошибок.
pub use error::{
    AclError, AuthError, ConfigError, NetworkError, ParseError, PasswordError, RecvError, Result,
    SlotManagerError, StoreError, StoreResult, TryRecvError, ZdbVersionError, ZspDecodeError,
    ZspEncodeError, ZspParserError, ZspSerializationError,
};
/// Реэкспорт API для работы с модулями и плагинами.
pub use modules::{DynamicModule, Manager, Module, Plugin, WasmPlugin};
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
