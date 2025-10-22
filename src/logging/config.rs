use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::logging::formats::{CustomFields, SpanConfig, TimestampConfig};

/// Формат вывода логов.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Pretty,
    Compact,
}

/// Политика ротации файлов логов.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RotationPolicy {
    Daily,
    Hourly,
    Size { mb: u64 },
    Never,
}

/// Стратегия именования файлов.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileNamingStrategy {
    /// zumic.log
    Simple,
    /// zumic-2025-10-06.log
    #[default]
    Dated,
    /// zumic-001.log
    Sequential,
    /// zumic-2025-10-06-001.log
    Full,
}

/// Конфигурация консольного вывода.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConsoleConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub format: Option<LogFormat>,
    #[serde(default = "default_true")]
    pub with_ansi: bool,
    #[serde(default = "default_true")]
    pub with_target: bool,
    #[serde(default = "default_false")]
    pub with_thread_ids: bool,
    #[serde(default = "default_false")]
    pub with_line_numbers: bool,
}

/// Конфигурация файлового вывода
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_filename")]
    pub filename: String,
    pub format: Option<LogFormat>,
    pub rotation: Option<RotationPolicy>,
    pub max_size_mb: Option<u64>,
    pub retention_days: Option<u32>,
    #[serde(default = "default_false")]
    pub compress_old: bool,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// File naming strategy
    #[serde(default)]
    pub naming: FileNamingStrategy,
    /// Автоматическое применение retention policy
    #[serde(default = "default_true")]
    pub auto_cleanup: bool,
    /// Интервал для background cleanup (секунды)
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_secs: u64,
}

/// Полная конфигурация системы логирования.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    /// Глобальный уровень логирования
    #[serde(default = "default_level")]
    pub level: String,
    /// Формат вывода логов
    #[serde(default)]
    pub format: LogFormat,
    /// Директория для файлов логов
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    /// Политика ротации файлов
    #[serde(default)]
    pub rotation: RotationPolicy,
    /// Включить консольный вывод
    #[serde(default = "default_true")]
    pub console_enabled: bool,
    /// Включить запись в файл
    #[serde(default = "default_true")]
    pub file_enabled: bool,
    /// Максимальный размер файла (МБ)
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: u64,
    /// Сколько дней хранить старые логи
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    /// Консольная конфигурация
    #[serde(default)]
    pub console: ConsoleConfig,
    /// Файловая конфигурация
    #[serde(default)]
    pub file: FileConfig,
    /// Per-module уровни логирования
    #[serde(default)]
    pub module_levels: Vec<String>,
    /// Custom fields (instance_id, version, environment, hostname)
    #[serde(default)]
    pub custom_fields: CustomFields,

    /// Timestamp configuration
    #[serde(default)]
    pub timestamp: TimestampConfig,

    /// Span configuration
    #[serde(default)]
    pub span: SpanConfig,

    /// Slow query logging configuration
    #[serde(default)]
    pub slow_log: SlowLogConfig,
}

/// Конфигурация slow query logging.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SlowLogConfig {
    /// Включить slow query logging
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Отдельный файл для slow queries
    #[serde(default = "default_slow_filename")]
    pub filename: String,
    /// Глобальный threshold (миллисекунды)
    #[serde(default = "default_slow_threshold")]
    pub threshold_ms: u64,
    /// Sample rate (0.0 - 1.0)
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
    /// Per-command thresholds (миллисекунды)
    #[serde(default)]
    pub command_thresholds: HashMap<String, u64>,
    /// Включить backtrace
    #[serde(default = "default_false")]
    pub enable_backtrace: bool,
    /// Максимальная длина args
    #[serde(default = "default_max_args")]
    pub max_args_len: usize,
}

impl LoggingConfig {
    /// Валидация конфигурации.
    pub fn validate(&self) -> Result<(), String> {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];

        if !valid_levels.contains(&self.level.as_str()) {
            return Err(format!(
                "Invalid log level '{}'. Valid: {:?}",
                self.level, valid_levels
            ));
        }

        if self.max_file_size_mb == 0 {
            return Err("max_file_size_mb must be > 0".to_string());
        }

        if self.retention_days == 0 {
            return Err("retention_days must be > 0".to_string());
        }

        for level_spec in &self.module_levels {
            if !level_spec.contains('=') {
                return Err(format!(
                    "Invalid module level '{}'. Expected: 'module::path=level'",
                    level_spec
                ));
            }
        }

        let console_enabled = self.console.enabled && self.console_enabled;
        let file_enabled = self.file.enabled && self.file_enabled;

        if !console_enabled && !file_enabled {
            return Err("At least one output must be enabled".to_string());
        }

        Ok(())
    }

    /// Применить environment variable overrides.
    pub fn apply_env_overrides(&mut self) {
        if let Ok(level) = std::env::var("ZUMIC_LOG_LEVEL") {
            self.level = level;
        }

        if let Ok(dir) = std::env::var("ZUMIC_LOG_DIR") {
            self.log_dir = PathBuf::from(dir);
        }

        if let Ok(format) = std::env::var("ZUMIC_LOG_FORMAT") {
            self.format = match format.to_lowercase().as_str() {
                "json" => LogFormat::Json,
                "pretty" => LogFormat::Pretty,
                "compact" => LogFormat::Compact,
                _ => self.format,
            };
        }

        if let Ok(val) = std::env::var("ZUMIC_LOG_CONSOLE") {
            self.console_enabled = val.parse().unwrap_or(self.console_enabled);
        }

        if let Ok(val) = std::env::var("ZUMIC_LOG_FILE") {
            self.file_enabled = val.parse().unwrap_or(self.file_enabled);
        }

        if self.module_levels.is_empty() {
            if let Ok(rust_log) = std::env::var("RUST_LOG") {
                self.module_levels = rust_log
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }

    /// Получить финальный формат для консолию.
    pub fn console_format(&self) -> LogFormat {
        self.console.format.unwrap_or(self.format)
    }

    /// Получить финальный формат для файла.
    pub fn file_format(&self) -> LogFormat {
        self.file.format.unwrap_or(self.format)
    }

    /// Получить финальную политику ротации.
    pub fn file_rotation(&self) -> RotationPolicy {
        self.file
            .rotation
            .clone()
            .unwrap_or_else(|| self.rotation.clone())
    }

    /// Построить EnvFilter директиву.
    pub fn build_filter_directive(&self) -> String {
        if self.module_levels.is_empty() {
            self.level.clone()
        } else {
            let mut directive = format!("zumic={}", self.level);
            for module_level in &self.module_levels {
                directive.push(',');
                directive.push_str(module_level);
            }
            directive
        }
    }

    /// Создать директорию для логов.
    pub fn ensure_log_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.log_dir)
    }

    /// Полный путь к файлу лога.
    pub fn log_file_path(&self) -> PathBuf {
        self.log_dir.join(&self.file.filename)
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            format: LogFormat::default(),
            log_dir: default_log_dir(),
            rotation: RotationPolicy::default(),
            console_enabled: true,
            file_enabled: true,
            max_file_size_mb: default_max_file_size(),
            retention_days: default_retention_days(),
            console: ConsoleConfig::default(),
            file: FileConfig::default(),
            module_levels: Vec::new(),
            custom_fields: CustomFields::default(),
            timestamp: TimestampConfig::default(),
            span: SpanConfig::default(),
            slow_log: SlowLogConfig::default(),
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for LogFormat {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        {
            LogFormat::Pretty
        }
        #[cfg(not(debug_assertions))]
        {
            LogFormat::Json
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for RotationPolicy {
    fn default() -> Self {
        RotationPolicy::Daily
    }
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: None,
            with_ansi: true,
            with_target: true,
            with_thread_ids: false,
            with_line_numbers: false,
        }
    }
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            filename: "zumic.log".to_string(),
            format: None,
            rotation: None,
            max_size_mb: None,
            retention_days: None,
            compress_old: false,
            buffer_size: 8192,
            naming: FileNamingStrategy::default(),
            auto_cleanup: true,
            cleanup_interval_secs: 3600,
        }
    }
}

impl Default for SlowLogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            filename: "slow-queries.log".to_string(),
            threshold_ms: 100,
            sample_rate: 1.0,
            command_thresholds: HashMap::new(),
            enable_backtrace: false,
            max_args_len: 256,
        }
    }
}

// Default value functions
fn default_level() -> String {
    "info".to_string()
}

fn default_log_dir() -> PathBuf {
    PathBuf::from("./logs")
}

fn default_max_file_size() -> u64 {
    100
}

fn default_retention_days() -> u32 {
    30
}

fn default_filename() -> String {
    "zumic.log".to_string()
}

fn default_buffer_size() -> usize {
    8192
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_cleanup_interval() -> u64 {
    3600 // 1 час
}

fn default_slow_filename() -> String {
    "slow-queries.log".to_string()
}

fn default_slow_threshold() -> u64 {
    100 // 100 мс
}

fn default_sample_rate() -> f64 {
    1.0 // Log all
}

fn default_max_args() -> usize {
    256
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use super::*;

    // Рекомендуется в Cargo.toml добавить в [dev-dependencies]: tempfile = "3"
    // Для простоты здесь используем std::env::temp_dir + уникальный суффикс.
    fn unique_tmp_dir(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("zumic_test_{}_{}", name, std::process::id()));
        p
    }

    /// Тест проверят, что уровень по умолчанию — "info"
    #[test]
    fn test_default_level_is_info() {
        let cfg = LoggingConfig::default();

        assert_eq!(cfg.level, "info");
    }

    /// Тест проверят, что `LoggingConfig::default()` содержит корректные
    /// значения по умолчанию.
    #[test]
    fn test_defaults() {
        let cfg = LoggingConfig::default();
        assert_eq!(cfg.level, "info");
        assert!(cfg.log_dir.ends_with("logs"));
        assert!(cfg.max_file_size_mb > 0);
        assert!(cfg.retention_days > 0);
        // console/file defaults
        assert!(cfg.console_enabled);
        assert!(cfg.file_enabled);
    }

    /// Тест проверят, что validate() возвращает Err для некорректных уровней и
    /// значений.
    #[test]
    fn test_validate_errors() {
        // Invalid level
        let cfg = LoggingConfig {
            level: "nope".to_string(),
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "Некорректный level должен провалить валидацию"
        );

        // max_file_size_mb == 0
        let cfg = LoggingConfig {
            level: "info".to_string(),
            max_file_size_mb: 0,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "max_file_size_mb == 0 должен провалить валидацию"
        );

        // retention_days == 0
        let cfg = LoggingConfig {
            retention_days: 0,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "retention_days == 0 должен провалить валидацию"
        );

        // module_levels без '='
        let cfg = LoggingConfig {
            retention_days: 30,
            module_levels: vec!["bad_directive".to_string()],
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "module_levels без '=' должен провалить валидацию"
        );
    }

    /// Тест проверят, что validate() успешен для корректной конфигурации.
    #[test]
    fn test_validate_success() {
        let cfg = LoggingConfig::default();
        assert!(cfg.validate().is_ok());
    }

    /// Тест проверят, что apply_env_overrides() корректно подхватывает env-vars
    /// и что изменение RUST_LOG заполняет module_levels, если оно было пустым.
    #[test]
    fn test_apply_env_overrides() {
        // Сохраним старые значения, чтобы вернуть
        let prev_level = env::var("ZUMIC_LOG_LEVEL").ok();
        let prev_dir = env::var("ZUMIC_LOG_DIR").ok();
        let prev_format = env::var("ZUMIC_LOG_FORMAT").ok();
        let prev_rust_log = env::var("RUST_LOG").ok();

        env::set_var("ZUMIC_LOG_LEVEL", "debug");
        env::set_var("ZUMIC_LOG_DIR", "/tmp/zumic_logs_test");
        env::set_var("ZUMIC_LOG_FORMAT", "json");
        env::set_var("RUST_LOG", "zumic::engine=debug,zumic=info");

        let mut cfg = LoggingConfig::default();
        cfg.module_levels.clear();
        cfg.apply_env_overrides();

        assert_eq!(cfg.level, "debug");
        assert_eq!(cfg.log_dir, PathBuf::from("/tmp/zumic_logs_test"));
        assert_eq!(cfg.format, LogFormat::Json);
        assert!(!cfg.module_levels.is_empty());

        // Restore env
        if let Some(v) = prev_level {
            env::set_var("ZUMIC_LOG_LEVEL", v);
        } else {
            env::remove_var("ZUMIC_LOG_LEVEL");
        }
        if let Some(v) = prev_dir {
            env::set_var("ZUMIC_LOG_DIR", v);
        } else {
            env::remove_var("ZUMIC_LOG_DIR");
        }
        if let Some(v) = prev_format {
            env::set_var("ZUMIC_LOG_FORMAT", v);
        } else {
            env::remove_var("ZUMIC_LOG_FORMAT");
        }
        if let Some(v) = prev_rust_log {
            env::set_var("RUST_LOG", v);
        } else {
            env::remove_var("RUST_LOG");
        }
    }

    /// Тест проверят, что console_format() и file_format() возвращают override,
    /// если `console.format`/`file.format` определены.
    #[test]
    fn test_console_and_file_format_override() {
        let cfg = LoggingConfig {
            format: LogFormat::Json,
            console: ConsoleConfig {
                format: Some(LogFormat::Pretty),
                ..Default::default()
            },
            file: FileConfig {
                format: Some(LogFormat::Compact),
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(cfg.console_format(), LogFormat::Pretty);
        assert_eq!(cfg.file_format(), LogFormat::Compact);
    }

    /// Тест проверят, что file_rotation() возвращает file.rotation если он
    /// задан, иначе — глобальную rotation.
    #[test]
    fn test_file_rotation_priority() {
        let cfg = LoggingConfig {
            rotation: RotationPolicy::Daily,
            file: FileConfig {
                rotation: Some(RotationPolicy::Hourly),
                ..Default::default()
            },
            ..Default::default()
        };

        // Проверка, что file.rotation имеет приоритет
        assert_eq!(cfg.file_rotation(), RotationPolicy::Hourly);

        // Убираем file.rotation и проверяем глобальную rotation
        let cfg = LoggingConfig {
            rotation: RotationPolicy::Daily,
            file: FileConfig::default(),
            ..Default::default()
        };
        assert_eq!(cfg.file_rotation(), RotationPolicy::Daily);
    }

    /// Тест проверят, что build_filter_directive() собирает корректную
    /// директиву.
    #[test]
    fn test_build_filter_directive() {
        let cfg = LoggingConfig {
            level: "warn".to_string(),
            module_levels: vec![
                "zumic::engine=debug".to_string(),
                "zumic::network=trace".to_string(),
            ],
            ..Default::default()
        };

        let directive = cfg.build_filter_directive();
        assert!(directive.starts_with("zumic=warn"));
        assert!(directive.contains("zumic::engine=debug"));
        assert!(directive.contains("zumic::network=trace"));
    }

    /// Тест проверят, что ensure_log_dir() создаёт директорию и log_file_path()
    /// корректно формирует путь.
    #[test]
    fn test_ensure_log_dir_and_log_path() {
        let tmp = unique_tmp_dir("logs");
        let _ = fs::remove_dir_all(&tmp);

        let cfg = LoggingConfig {
            log_dir: tmp.clone(),
            file: FileConfig {
                filename: "test_zumic.log".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(cfg.ensure_log_dir().is_ok());
        assert!(tmp.exists() && tmp.is_dir());

        let path = cfg.log_file_path();
        assert!(path.ends_with("test_zumic.log"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
