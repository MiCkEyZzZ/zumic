use std::path::PathBuf;

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
            level: default_filename(),
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
