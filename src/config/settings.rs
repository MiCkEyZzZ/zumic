use std::net::SocketAddr;

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

/// Тип хранилища, используемого сервером.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    /// Данные хранятся только в памяти (самый быстрый режим).
    Memory,
    /// Данные сохраняются на диске (постоянное хранилище).
    Persistent,
    /// Кластерное распределённое хранилище.
    Cluster,
}

/// Конфигурация движка хранения (StorageEngine).
#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub storage_type: StorageType,
}

impl StorageConfig {
    /// Создаёт конфиг StorageEngine на основе глобальных настроек приложения.
    pub fn new(settings: &Settings) -> Self {
        StorageConfig {
            storage_type: settings.storage_type.clone(),
        }
    }
}

// --- Defaults for Serde (используются при отсутствии значения в конфиге) ---

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

/// Десериализация SocketAddr из строки.
fn de_socket_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

fn default_storage_str() -> &'static str {
    "memory"
}

fn default_log_level_str() -> &'static str {
    "info"
}

/// Все параметры приложения в одном месте.
///
/// Значения берутся из:
/// 1. `config/default.toml`
/// 2. `config/<RUST_ENV>.toml`
/// 3. Переменные окружения `ZUMIC_*`
///
/// Используется для настройки сервера, хранилища, пулов потоков и таймаутов.
#[derive(Debug, Deserialize)]
pub struct Settings {
    /// Адрес и порт для прослушивания TCP-соединений.
    #[serde(deserialize_with = "de_socket_addr", default = "default_listen")]
    pub listen_address: SocketAddr,

    /// Максимальное число одновременно открытых соединений.
    #[serde(default = "default_max_connections")]
    pub max_connections: i64,

    /// Тип хранилища данных.
    #[serde(default = "default_storage")]
    pub storage_type: StorageType,

    /// Максимальное число соединений с одного IP (защита от DoS).
    #[serde(default)]
    pub max_connections_per_ip: Option<usize>,

    /// Таймаут бездействия соединения в секундах.
    #[serde(default)]
    pub connection_timeout: Option<u64>,

    /// Таймаут чтения команды от клиента.
    #[serde(default)]
    pub read_timeout: Option<u64>,

    /// Таймаут записи ответа клиенту.
    #[serde(default)]
    pub write_timeout: Option<u64>,

    /// Размер буфера чтения на соединение (байт).
    #[serde(default)]
    pub read_buffer_size: Option<usize>,

    /// Таймаут завершения работы сервера (graceful shutdown) в секундах.
    #[serde(default)]
    pub shutdown_timeout: Option<u64>,

    /// Путь до AOF-файла (Append Only File) для персистентности.
    #[serde(default)]
    pub aof_path: Option<String>,

    /// Частота создания снапшотов (секунды).
    #[serde(default)]
    pub snapshot_freq: Option<u64>,

    /// Лимит потребления памяти (строка с суффиксом K/M/G, например "512MB").
    #[serde(default)]
    pub max_memory: Option<String>,

    /// Уровень логирования: "trace", "debug", "info", "warn", "error".
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Количество потоков в пуле для асинхронных задач.
    #[serde(default = "num_cpus::get")]
    pub thread_pool_size: usize,
}

impl Settings {
    /// Загружает конфигурацию приложения.
    ///
    /// Порядок загрузки:
    /// 1. `config/default.toml` (необязательно)
    /// 2. `config/<RUST_ENV>.toml` (необязательно)
    /// 3. Переменные окружения с префиксом `ZUMIC_`
    ///
    /// Возвращает `Settings` или ошибку `ConfigError`.
    pub fn load() -> Result<Self, ConfigError> {
        let profile = std::env::var("RUST_ENV").unwrap_or_else(|_| "dev".into());
        let builder = Config::builder()
            .add_source(File::with_name("src/config/default").required(false))
            .add_source(File::with_name(&format!("src/config/{profile}")).required(false))
            .add_source(Environment::with_prefix("ZUMIC").separator("_"))
            .set_default("listen_address", default_listen().to_string())?
            .set_default("max_connections", default_max_connections())?
            .set_default("storage_type", default_storage_str())?
            .set_default("log_level", default_log_level_str())?;

        builder.build()?.try_deserialize()
    }
}
