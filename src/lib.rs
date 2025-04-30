/// HTTP-админка: routes, handlers и состояние.
pub mod admin;
pub mod application;
/// Аутентификация и ACL: пользователи, пароли, правила доступа.
pub mod auth;
/// Парсер и исполнение command-строк (SET, GET и т.д.).
pub mod command;
/// Загрузка конфигурации сервера.
pub mod config;
/// Встроенные структуры данных (Dict, SkipList, QuickList, SDS).
pub mod database;
/// Абстракция движка хранения и реализации (InMemory, Persistent, Cluster).
pub mod engine;
/// Общие ошибки: кодирование/декодирование, парсинг, хранилище.
pub mod error;
/// Гибкое логирование (форматирование, фильтры, sinks).
pub mod logging;
/// Сетевой стек: протокол ZSP и сервер на Tokio.
pub mod network;
/// Pub/Sub: Broker, Subscription, Message.
pub mod pubsub;

// -----------------------------------------------------------------------------
//  Часто используемые публичные типы
// -----------------------------------------------------------------------------

pub use application::{AclPort, CommandExecute, PubSubPort, StoragePort, SubscriptionPort};
/// Функции хеширования и проверки паролей, ACL-менеджер.
pub use auth::{
    hash_password, verify_password, Acl, AclRule, AclUser, AuthManager, CmdCategory, ServerConfig,
    UserConfig,
};
/// Основные команды key-value: SET, GET, INCR, HSET, LPOP, ZADD и др.
pub use command::{
    AppendCommand, AuthCommand, Command, DecrByCommand, DecrByFloatCommand, DecrCommand,
    DelCommand, ExistsCommand, FlushDbCommand, GetCommand, GetRangeCommand, HDelCommand,
    HGetAllCommand, HGetCommand, HSetCommand, IncrByCommand, IncrByFloatCommand, IncrCommand,
    LLenCommand, LPopCommand, LPushCommand, LRangeCommand, MGetCommand, MSetCommand, RPopCommand,
    RPushCommand, RenameCommand, RenameNxCommand, SAddCommand, SCardCommand, SIsMemberCommand,
    SMembersCommand, SRemCommand, SetCommand, SetFloatCommand, SetNxCommand, StrLenCommand,
    ZAddCommand, ZCardCommand, ZRangeCommand, ZRemCommand, ZRevRangeCommand, ZScoreCommand,
};
/// Типы данных: Dict, QuickList, SkipList, Sds и другие.
pub use database::{Dict, QuickList, Sds, SkipList, SmartHash, Value};
/// Движки хранения: InMemoryStore, PersistentStore, ClusterStore.
pub use engine::{ClusterStore, InMemoryStore, PersistentStore, StorageEngine};
/// Ошибки и результаты операций.
pub use error::{
    AclError, AuthError, ConfigError, DecodeError, EncodeError, NetworkError, ParseError,
    PasswordError, StoreError, StoreResult,
};
/// Сетевой сервер и протокол.
pub use network::{server, zsp};
/// API Pub/Sub.
pub use pubsub::{Broker, Message, Subscription};
