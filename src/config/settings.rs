use serde::{Deserialize, Serialize};

use config::{Config, ConfigError, Environment};

#[derive(Debug, Clone)]
pub enum StorageType {
    Memory,
}

/// Storage Configuration.
pub struct StorageConfig {
    pub storage_type: StorageType,
    // pub storage_path: Option<String>,       // For file storage
    // pub cache_size: Option<usize>,          // LRU cache size
    // pub balancing_strategy: Option<String>, // Balancing strategy
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
            // Adding default values
            .set_default("listen_address", "127.0.0.1:6379")?
            .set_default("max_connections", 100)?
            // Add enviroment variables with the ZUMIC_
            .add_source(Environment::with_prefix("ZUMIC"))
            .build()?;

        // Seserialize the configuration into our structure.
        cfg.try_deserialize()
    }
}
