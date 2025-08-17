use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    Memory,
    Persistent,
    Cluster,
}

/// Конфиг для StorageEngine
#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub storage_type: StorageType,
}

impl StorageConfig {
    /// Создаёт StorageConfig из загруженных настроек
    pub fn new(settings: &Settings) -> Self {
        StorageConfig {
            storage_type: settings.storage_type.clone(),
        }
    }
}

// --- defaults for serde:

fn default_listen() -> SocketAddr {
    "127.0.0.1:6174".parse().unwrap()
}

fn default_max_connections() -> i64 {
    100
}

fn default_storage() -> StorageType {
    StorageType::Memory
}

fn default_log_level() -> String {
    "info".into()
}

fn de_socket_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

// --- defaults for Config::builder():

fn default_storage_str() -> &'static str {
    "memory"
}

fn default_log_level_str() -> &'static str {
    "info"
}

/// Все параметры приложения в одном месте
#[derive(Debug, Deserialize)]
pub struct Settings {
    /// Адрес и порт, на которых слушать
    #[serde(deserialize_with = "de_socket_addr", default = "default_listen")]
    pub listen_address: SocketAddr,

    /// Максимум соединений
    #[serde(default = "default_max_connections")]
    pub max_connections: i64,

    /// Тип хранилища: memory, persistent или cluster
    #[serde(default = "default_storage")]
    pub storage_type: StorageType,

    /// Путь до AOF (Append Only File)
    #[serde(default)]
    pub aof_path: Option<String>,

    /// Частота снапшотов (сек)
    #[serde(default)]
    pub snapshot_freq: Option<u64>,

    /// Лимит памяти (например "512MB")
    #[serde(default)]
    pub max_memory: Option<String>,

    /// Уровень логирования
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Пул потоков для тяжёлых задач
    #[serde(default = "num_cpus::get")]
    pub thread_pool_size: usize,
}

impl Settings {
    /// Загружает конфиг из:
    /// 1) config/default.toml
    /// 2) config/<RUST_ENV>.toml
    /// 3) переменных ZUMIC_*
    pub fn load() -> Result<Self, ConfigError> {
        let profile = std::env::var("RUST_ENV").unwrap_or_else(|_| "dev".into());

        let builder = Config::builder()
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{profile}")).required(false))
            .add_source(Environment::with_prefix("ZUMIC").separator("_"))
            .set_default("listen_address", default_listen().to_string())?
            .set_default("max_connections", default_max_connections())?
            .set_default("storage_type", default_storage_str())?
            .set_default("log_level", default_log_level_str())?;

        builder.build()?.try_deserialize()
    }
}
