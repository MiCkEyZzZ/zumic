pub mod compact;
pub mod json;
pub mod pretty;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimestampFormat {
    /// RFC3339 (ISO 8601): "2025-10-04T12:34:56.789Z"
    #[default]
    Rfc3339,
    /// Unix timestamp (seconds since epoch)
    Unix,
    /// Custom format string (strftime)
    Custom,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Timezone {
    #[default]
    Utc,
    Local,
}

/// Custom fields добавляемые ко всем логам.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomFields {
    /// Instance ID (для различения инстансов в кластере)
    #[serde(default)]
    pub instance_id: Option<String>,
    /// Version приложения
    #[serde(default)]
    pub version: Option<String>,
    /// Environment (dev/staging/production)
    #[serde(default)]
    pub environment: Option<String>,
    /// Hostname
    #[serde(default)]
    pub hostname: Option<String>,
}

/// Конфигурация для timestamp.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct TimestampConfig {
    /// Формат timestamp (RFC3339, Unix, Custom)
    #[serde(default)]
    pub format: TimestampFormat,
    /// Timezone (UTC, Local)
    #[serde(default)]
    pub timezone: Timezone,
}

/// Конфигурация для span fields
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpanConfig {
    /// Включить span name
    #[serde(default = "default_true")]
    pub include_name: bool,

    /// Включить span fields
    #[serde(default = "default_true")]
    pub include_fields: bool,

    /// Включить full span list (all parent spans)
    #[serde(default = "default_false")]
    pub include_full_list: bool,

    /// Максимальная глубина span tree
    #[serde(default = "default_span_depth")]
    pub max_depth: usize,
}

impl Default for CustomFields {
    fn default() -> Self {
        Self {
            instance_id: std::env::var("ZUMIC_INSTANCE_ID").ok(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            environment: std::env::var("ZUMIC_ENV")
                .or_else(|_| std::env::var("RUST_ENV"))
                .ok(),
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
        }
    }
}

impl Default for SpanConfig {
    fn default() -> Self {
        Self {
            include_name: true,
            include_fields: true,
            include_full_list: false,
            max_depth: 5,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_span_depth() -> usize {
    5
}
