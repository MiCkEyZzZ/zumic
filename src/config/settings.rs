use serde::{Deserialize, Serialize};

use config::{Config, ConfigError, Environment};

#[derive(Debug, Clone)]
pub enum StorageType {
    Memory,
    Cluster,
    Persistent,
}

/// Storage Configuration.
pub struct StorageConfig {
    pub storage_type: StorageType,
    // pub storage_path: Option<String>,       // Для хранения файлов
    // pub cache_size: Option<usize>,          // Размер кэша LRU
    // pub balancing_strategy: Option<String>, // Балансирующая стратегия
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub listen_add: String,
    pub aof_path: Option<String>,
    pub max_connections: usize,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        let cfg = Config::builder()
            // Добавление значений по умолчанию
            .set_default("listen_address", "127.0.0.1:6379")?
            .set_default("max_connections", 100)?
            // Добавьте переменные окружения с помощью ZUMIC_
            .add_source(Environment::with_prefix("ZUMIC"))
            .build()?;

        // Сериализуем конфигурацию в нашу структуру.
        cfg.try_deserialize()
    }
}
