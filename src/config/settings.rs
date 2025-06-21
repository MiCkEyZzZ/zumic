use config::{Config, ConfigError, Environment};
use serde::Deserialize;

/// Тип хранилища
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    Memory,
    Cluster,
    Persistent,
}

/// Конфиг для storage
#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub storage_type: StorageType,
    // можно добавить path, cache и т.д.
}

/// Общий конфиг приложения
#[derive(Debug, Deserialize)]
pub struct Settings {
    #[serde(rename = "listen_address")]
    pub listen_address: String, // "127.0.0.1:6379"
    pub max_connections: usize, // не используем в примере
    #[serde(default = "default_storage_type")]
    pub storage_type: StorageType,
}

fn default_storage_type() -> StorageType {
    StorageType::Memory
}

impl Settings {
    pub fn load() -> Result<(Self, StorageConfig), ConfigError> {
        let cfg = Config::builder()
            .set_default("listen_address", "127.0.0.1:6379")?
            .set_default("max_connections", 100)?
            // читаем тип storage из переменных, например ZUMIC_STORAGE_TYPE=persistent
            .add_source(Environment::with_prefix("ZUMIC").separator("_"))
            .build()?;
        let settings: Settings = cfg.try_deserialize()?;

        // трансформируем в StorageConfig
        let storage_cfg = StorageConfig {
            storage_type: settings.storage_type.clone(),
        };
        Ok((settings, storage_cfg))
    }
}
