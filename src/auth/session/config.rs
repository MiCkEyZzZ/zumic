use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub ttl: Duration,
    pub max_sessions_per_user: Option<usize>,
    pub validate_ip: bool,
    pub cleanup_interval: Duration,
}

#[derive(Debug, Default)]
pub struct SessionConfigBuilder {
    ttl: Option<Duration>,
    max_session_per_user: Option<Option<usize>>,
    validate_ip: Option<bool>,
    cleanup_interval: Option<Duration>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl SessionConfig {
    pub fn builder() -> SessionConfigBuilder {
        SessionConfigBuilder::default()
    }
}

impl SessionConfigBuilder {
    pub fn ttl(
        mut self,
        ttl: Duration,
    ) -> Self {
        self.ttl = Some(ttl);
        self
    }

    pub fn max_sessions_per_user(
        mut self,
        max: usize,
    ) -> Self {
        self.max_session_per_user = Some(Some(max));
        self
    }

    pub fn unlimited_sessions(mut self) -> Self {
        self.max_session_per_user = Some(None);
        self
    }

    pub fn validate_ip(
        mut self,
        validate: bool,
    ) -> Self {
        self.validate_ip = Some(validate);
        self
    }

    pub fn cleanup_interval(
        mut self,
        interval: Duration,
    ) -> Self {
        self.cleanup_interval = Some(interval);
        self
    }

    pub fn build(self) -> SessionConfig {
        let default = SessionConfig::default();
        SessionConfig {
            ttl: self.ttl.unwrap_or(default.ttl),
            max_sessions_per_user: self
                .max_session_per_user
                .unwrap_or(default.max_sessions_per_user),
            validate_ip: self.validate_ip.unwrap_or(default.validate_ip),
            cleanup_interval: self.cleanup_interval.unwrap_or(default.cleanup_interval),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для SessionConfig
////////////////////////////////////////////////////////////////////////////////

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(3600),             // 1ч
            max_sessions_per_user: Some(5),             // максимум 5 сессий
            validate_ip: true,                          // проверяем IP
            cleanup_interval: Duration::from_secs(300), // очистка каждые 5 минут
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SessionConfig::default();
        assert_eq!(config.ttl, Duration::from_secs(3600));
        assert_eq!(config.max_sessions_per_user, Some(5));
        assert!(config.validate_ip);
        assert_eq!(config.cleanup_interval, Duration::from_secs(300));
    }

    #[test]
    fn test_builder_full_customization() {
        let config = SessionConfig::builder()
            .ttl(Duration::from_secs(7200))
            .max_sessions_per_user(10)
            .validate_ip(false)
            .cleanup_interval(Duration::from_secs(600))
            .build();

        assert_eq!(config.ttl, Duration::from_secs(7200));
        assert_eq!(config.max_sessions_per_user, Some(10));
        assert!(!config.validate_ip);
        assert_eq!(config.cleanup_interval, Duration::from_secs(600));
    }

    #[test]
    fn test_builder_partial() {
        let config = SessionConfig::builder()
            .ttl(Duration::from_secs(1800))
            .build();

        assert_eq!(config.ttl, Duration::from_secs(1800));
        // Остальные параметры из default
        assert_eq!(config.max_sessions_per_user, Some(5));
        assert!(config.validate_ip);
    }

    #[test]
    fn test_builder_unlimited_sessions() {
        let config = SessionConfig::builder().unlimited_sessions().build();

        assert_eq!(config.max_sessions_per_user, None);
    }
}
