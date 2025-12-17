//! Конфигурация приложения Zumic.
//!
//! Модуль отвечает за:
//! - загрузку конфигурации из файлов и переменных окружения,
//! - объединение настроек из разных источников,
//! - предоставление типобезопасного API для доступа к настройкам,
//! - валидацию критичных параметров (например, логирования).

use std::net::SocketAddr;

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

use crate::logging::config::LoggingConfig;

/// Тип хранилища, используемого сервером.
///
/// Определяет стратегию хранения данных и влияет на производительность,
/// отказоустойчивость и требования к инфраструктуре.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    /// Данные хранятся только в памяти.
    ///
    /// Максимальная производительность, но без персистентности.
    Memory,
    /// Данные сохраняются на диск (AOF / снапшоты).
    ///
    /// Баланс между производительностью и надёжностью.
    Persistent,
    /// Распределённое кластерное хранилище.
    ///
    /// Предназначено для горизонтального масштабирования.
    Cluster,
}

/// Конфигурация движка хранения (StorageEngine).
///
/// Это производная структура, которая формируется на основе глобальных
/// настроек [`Settings`] и используется внутри слоя хранения.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Тип используемого хранилища.
    pub storage_type: StorageType,
}

impl StorageConfig {
    /// Создаёт конфигурацию хранилища на основе глобальных настроек приложения.
    pub fn new(settings: &Settings) -> Self {
        StorageConfig {
            storage_type: settings.storage_type.clone(),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Настройки по умолчанию для Serde
////////////////////////////////////////////////////////////////////////////////

/// Адрес по умолчанию для TCP-сервера.
///
/// Используется, если параметр не задан ни в файлах конфигурации,
/// ни через переменные окружения.
fn default_listen() -> SocketAddr {
    "127.0.0.1:6174".parse().unwrap()
}

/// Максимальное число соединений по умолчанию.
fn default_max_connections() -> i64 {
    100
}

/// Тип хранилища по умолчанию.
fn default_storage() -> StorageType {
    StorageType::Memory
}

/// Уровень логирования по умолчанию (legacy).
fn default_log_level() -> String {
    "info".into()
}

/// Десериализация [`SocketAddr`] из строки.
///
/// Используется для поддержки формата:
/// ```toml
/// listen_address = "127.0.0.1:6174"
/// ```
fn de_socket_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

/// Строковое представление типа хранилища по умолчанию.
fn default_storage_str() -> &'static str {
    "memory"
}

/// Строковое представление уровня логирования по умолчанию.
fn default_log_level_str() -> &'static str {
    "info"
}

////////////////////////////////////////////////////////////////////////////////
// Настройки
////////////////////////////////////////////////////////////////////////////////

/// Глобальная конфигурация приложения.
///
/// Все параметры сервера, хранилища, сетевого слоя и логирования
/// собраны в одной структуре.
///
/// ## Загрузка значений
/// Значения объединяются из следующих источников (по приоритету):
/// 1. `config/default.toml`
/// 2. `config/<RUST_ENV>.toml`
/// 3. Переменные окружения `ZUMIC_*`
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

    /// Уровень логирования (legacy).
    ///
    /// ⚠️ **DEPRECATED**: используйте `logging.level`.
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Количество потоков в пуле для асинхронных задач.
    #[serde(default = "num_cpus::get")]
    pub thread_pool_size: usize,

    /// Конфигурация логирования (новая, расширенная)
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Settings {
    /// Загружает и валидирует конфигурацию приложения.
    ///
    /// Автоматически:
    /// - определяет профиль окружения (`RUST_ENV`, по умолчанию `dev`);
    /// - объединяет настройки из файлов и env;
    /// - применяет обратную совместимость `log_level → logging.level`;
    /// - выполняет валидацию конфигурации логирования.
    ///
    /// ## Ошибки
    /// Возвращает [`ConfigError`], если:
    /// - конфигурация некорректна,
    /// - значения имеют неверный формат,
    /// - не проходит валидация логирования.
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

        let mut settings: Settings = builder.build()?.try_deserialize()?;

        // Обратная совместимость: если logging.level пустой, используем log_level
        if settings.logging.level == "info" && settings.log_level != "info" {
            settings.logging.level = settings.log_level.clone();
        }

        // Применяем env overrides для логирования
        settings.logging.apply_env_overrides();

        // Валидация логирования
        settings
            .logging
            .validate()
            .map_err(|e| ConfigError::Message(format!("Logging config validation failed: {e}")))?;

        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, net::SocketAddr};

    use tempfile::TempDir;

    use super::*;
    use crate::{Settings, StorageType};

    /// Очищает все env-переменные с префиксом ZUMIC перед тестом
    fn clear_zumic_env() {
        for (key, _) in env::vars() {
            if key.starts_with("ZUMIC_") {
                env::remove_var(&key);
            }
        }
        env::remove_var("RUST_ENV");
    }

    /// Создаёт минимальную конфигурацию без зависимости от файлов
    fn load_minimal_settings() -> Result<Settings, ConfigError> {
        let builder = Config::builder()
            .set_default("listen_address", default_listen().to_string())?
            .set_default("max_connections", default_max_connections())?
            .set_default("storage_type", default_storage_str())?
            .set_default("log_level", default_log_level_str())?;

        let settings: Settings = builder.build()?.try_deserialize()?;

        Ok(settings)
    }

    /// Тест проверяет дефолтный конфиг и serde дефолт
    #[test]
    fn test_default_settings() {
        clear_zumic_env();

        let settings = load_minimal_settings().expect("Failed to load default settings");

        assert_eq!(
            settings.listen_address,
            "127.0.0.1:6174".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(settings.max_connections, 100);
        assert!(matches!(settings.storage_type, StorageType::Memory));
        assert_eq!(settings.log_level, "info");
        assert_eq!(settings.thread_pool_size, num_cpus::get());
    }

    /// Тест проверяет десериализации SocketAddr
    #[test]
    fn test_socket_addr_deserialization() {
        let s = "127.0.0.1:8080".parse::<SocketAddr>().unwrap();
        assert_eq!(s.to_string(), "127.0.0.1:8080");
    }

    /// Тест проверяет загрузки профиля (cluster/memory/persistent)
    #[test]
    fn test_profile_override() {
        clear_zumic_env();

        let temp_dir = TempDir::new().unwrap();
        let profile_path = temp_dir.path().join("test_cluster.toml");
        fs::write(
            &profile_path,
            r#"
                listen_address = "0.0.0.0:9000"
                storage_type = "cluster"
                max_connections = 500
            "#,
        )
        .unwrap();

        let builder = Config::builder()
            .set_default("listen_address", default_listen().to_string())
            .unwrap()
            .set_default("max_connections", default_max_connections())
            .unwrap()
            .set_default("storage_type", default_storage_str())
            .unwrap()
            .set_default("log_level", default_log_level_str())
            .unwrap()
            .add_source(File::with_name(&profile_path.to_string_lossy()).required(true));

        let settings: Settings = builder.build().unwrap().try_deserialize().unwrap();

        assert_eq!(
            settings.listen_address,
            "0.0.0.0:9000".parse::<SocketAddr>().unwrap()
        );
        assert!(matches!(settings.storage_type, StorageType::Cluster));
        assert_eq!(settings.max_connections, 500);
    }

    /// Тест проверяет env override (используем set_override вместо реальных
    /// env-переменных)
    #[test]
    fn test_env_override() {
        clear_zumic_env();

        // Используем set_override для эмуляции env-переменных
        // Это более надёжно, чем реальные env::set_var в тестах
        let builder = Config::builder()
            .set_default("listen_address", default_listen().to_string())
            .unwrap()
            .set_default("max_connections", default_max_connections())
            .unwrap()
            .set_default("storage_type", default_storage_str())
            .unwrap()
            .set_default("log_level", default_log_level_str())
            .unwrap()
            // Эмулируем переопределение через env-переменные
            .set_override("listen_address", "127.0.0.1:9999")
            .unwrap()
            .set_override("max_connections", 777)
            .unwrap()
            .set_override("storage_type", "persistent")
            .unwrap();

        let settings: Settings = builder.build().unwrap().try_deserialize().unwrap();

        assert_eq!(
            settings.listen_address,
            "127.0.0.1:9999".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(settings.max_connections, 777);
        assert!(matches!(settings.storage_type, StorageType::Persistent));
    }

    /// Тест проверяет обратную совместимость log_level → logging.level
    #[test]
    fn test_logging_level_compatibility() {
        clear_zumic_env();

        let builder = Config::builder()
            .set_override("log_level", "debug")
            .unwrap()
            .set_default("listen_address", default_listen().to_string())
            .unwrap()
            .set_default("max_connections", default_max_connections())
            .unwrap()
            .set_default("storage_type", default_storage_str())
            .unwrap();

        let mut settings: Settings = builder.build().unwrap().try_deserialize().unwrap();

        // Имитируем логику из Settings::load()
        if settings.logging.level == "info" && settings.log_level != "info" {
            settings.logging.level = settings.log_level.clone();
        }

        assert_eq!(settings.logging.level, "debug");
    }

    /// Тест проверяет применение env overrides для логирования
    #[test]
    fn test_logging_env_override() {
        clear_zumic_env();

        // Используем set_override для надёжного тестирования
        let builder = Config::builder()
            .set_default("listen_address", default_listen().to_string())
            .unwrap()
            .set_default("max_connections", default_max_connections())
            .unwrap()
            .set_default("storage_type", default_storage_str())
            .unwrap()
            .set_default("log_level", default_log_level_str())
            .unwrap()
            .set_override("logging.level", "warn")
            .unwrap();

        let settings: Settings = builder.build().unwrap().try_deserialize().unwrap();

        assert_eq!(settings.logging.level, "warn");
    }

    /// Тест проверяет валидацию логирования
    #[test]
    fn test_logging_validation() {
        clear_zumic_env();

        let mut settings = load_minimal_settings().unwrap();
        settings.logging.level = "info".into();
        let res = settings.logging.validate();
        assert!(res.is_ok());
    }
}
