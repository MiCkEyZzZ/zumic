use serde::{Deserialize, Serialize};

use config::{Config, ConfigError, Environment};

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub listen_add: String,
    pub aof_path: Option<String>,
    pub max_connections: usize,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        let cfg = Config::builder()
            // Добавляем значения по умолчанию
            .set_default("listen_address", "127.0.0.1:6379")?
            .set_default("max_connections", 100)?
            // Добавляем переменные окружения с префиксом ZUMIC_
            .add_source(Environment::with_prefix("ZUMIC"))
            .build()?;

        // Десериализуем конфигурацию в нашу структуру
        cfg.try_deserialize()
    }
}
