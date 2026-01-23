use std::{fmt, str::FromStr, time::Duration};

use tokio::time::Instant;
use uuid::Uuid;
use zumic_error::SessionError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

#[derive(Debug, Clone)]
pub struct SessionData {
    pub username: String,
    pub ip_address: Option<String>,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub expires_at: Instant,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl SessionData {
    pub fn new(
        username: impl Into<String>,
        ip_address: Option<String>,
        ttl: Duration,
    ) -> Self {
        let now = Instant::now();
        Self {
            username: username.into(),
            ip_address,
            created_at: now,
            last_activity: now,
            expires_at: now + ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    pub fn update_activity(
        &mut self,
        ttl: Duration,
    ) {
        self.last_activity = Instant::now();
        self.expires_at = self.last_activity + ttl;
    }

    pub fn validate_ip(
        &self,
        ip: Option<&str>,
    ) -> bool {
        match (&self.ip_address, ip) {
            (Some(session_ip), Some(request_ip)) => session_ip == request_ip,
            (None, _) | (_, None) => true, // если IP не записан, пропускаем проверку
        }
    }

    pub fn time_until_expiry(&self) -> Duration {
        self.expires_at
            .checked_duration_since(Instant::now())
            .unwrap_or(Duration::ZERO)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для SessionId
////////////////////////////////////////////////////////////////////////////////

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = SessionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(SessionId)
            .map_err(|_| SessionError::InvalidSessionId)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_creation_and_parsing() {
        let id = SessionId::new();
        let id_str = id.to_string();
        let parsed = SessionId::from_str(&id_str).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_invalid_session_id_parsing() {
        assert!(SessionId::from_str("not-a-uuid").is_err());
        assert!(SessionId::from_str("").is_err());
    }

    #[test]
    fn test_session_data_expiration() {
        let ttl = Duration::from_secs(10);
        let mut session = SessionData::new("anton", None, ttl);

        // сразу не истекла
        assert!(!session.is_expired());

        // искусственно "перематываем" expire_at в прошлое
        session.expires_at = Instant::now() - Duration::from_secs(1);
        assert!(session.is_expired());
    }

    #[test]
    fn test_ip_validation() {
        let session = SessionData::new("anton", Some("127.0.0.1".into()), Duration::from_secs(10));

        // правильный IP
        assert!(session.validate_ip(Some("127.0.0.1")));

        // неправильный IP
        assert!(!session.validate_ip(Some("192.168.1.1")));

        // если в запросе нет IP, но в сессии есть - все равно true
        assert!(session.validate_ip(None));
    }

    #[test]
    fn test_ip_validation_no_ip_stored() {
        let session = SessionData::new("anton", None, Duration::from_secs(10));

        // если IP не записан, любой валиден
        assert!(session.validate_ip(Some("127.0.0.1")));
        assert!(session.validate_ip(Some("192.168.1.1")));
        assert!(session.validate_ip(None));
    }

    #[test]
    fn test_time_until_expiry() {
        let ttl = Duration::from_secs(60);
        let session = SessionData::new("anton", None, ttl);

        let remaining = session.time_until_expiry();

        // должно быть примерно 60 секунд (с небольшой погрешностью)
        assert!(remaining > Duration::from_secs(59));
        assert!(remaining <= ttl);
    }
}
