use std::{
    fmt,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;
use zumic_error::SessionError;

use crate::auth::TokenClaims;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionData {
    pub username: String,
    pub token_claims: Option<TokenClaims>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub created_at: Instant,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub last_activity: Instant,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
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
            token_claims: None,
            ip_address,
            user_agent: None,
            created_at: now,
            last_activity: now,
            expires_at: now + ttl,
        }
    }

    pub fn new_with_token(
        username: impl Into<String>,
        token_claims: TokenClaims,
        ip_address: Option<String>,
        user_agent: Option<String>,
        ttl: Duration,
    ) -> Self {
        let now = Instant::now();
        Self {
            username: username.into(),
            token_claims: Some(token_claims),
            ip_address,
            user_agent,
            created_at: now,
            last_activity: now,
            expires_at: now + ttl,
        }
    }

    pub fn new_full(
        username: impl Into<String>,
        token_id: Option<String>,
        ip_address: Option<String>,
        user_agent: Option<String>,
        ttl: Duration,
    ) -> Self {
        let now = Instant::now();
        let username = username.into();
        // Если передан token_id, создаём базовый TokenClaims только с jti
        let token_claims = token_id.map(|jti| TokenClaims {
            jti,
            sub: username.clone(),
            permissions: String::new(),
            iat: 0,
            exp: 0,
            token_type: "access".to_string(),
        });

        Self {
            username,
            token_claims,
            ip_address,
            user_agent,
            created_at: now,
            last_activity: now,
            expires_at: now + ttl,
        }
    }

    pub fn token_id(&self) -> Option<&str> {
        self.token_claims.as_ref().map(|c| c.jti.as_str())
    }

    pub fn permissions(&self) -> Option<&str> {
        self.token_claims.as_ref().map(|c| c.permissions.as_str())
    }

    pub fn is_token_expired(&self) -> bool {
        if let Some(claims) = &self.token_claims {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            claims.exp < now
        } else {
            false
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

    pub fn validate_user_agent(
        &self,
        user_agent: Option<&str>,
    ) -> bool {
        match (&self.user_agent, user_agent) {
            (Some(session_ua), Some(request_ua)) => session_ua == request_ua,
            (None, _) | (_, None) => true,
        }
    }

    pub fn validate_fingerprint(
        &self,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> bool {
        self.validate_ip(ip) && self.validate_user_agent(user_agent)
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

fn serialize_instant<S>(
    instant: &Instant,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    // Сохраняем как миллисекунды с момента старта программы
    // Для production лучше использовать SystemTime + UNIX_EPOCH
    let millis = instant.elapsed().as_millis();
    serializer.serialize_u128(millis)
}

fn deserialize_instant<'de, D>(deserializer: D) -> Result<Instant, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let millis = u128::deserialize(deserializer)?;
    // Восстанавливаем относительно текущего момента
    // ВАЖНО: для Redis нужно использовать абсолютное время (SystemTime)
    Ok(Instant::now() - Duration::from_millis(millis as u64))
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
    fn test_new_with_token() {
        let claims = TokenClaims {
            jti: "token_123".into(),
            sub: "anton".into(),
            permissions: "+@read,+get".into(),
            iat: 1000,
            exp: 2000,
            token_type: "access".into(),
        };

        let session = SessionData::new_with_token(
            "anton",
            claims.clone(),
            Some("127.0.0.1".into()),
            Some("Chrome/100".into()),
            Duration::from_secs(3600),
        );

        assert_eq!(session.username, "anton");
        assert_eq!(session.token_id().unwrap(), "token_123");
        assert_eq!(session.permissions().unwrap(), "+@read,+get");
        assert_eq!(session.ip_address.unwrap(), "127.0.0.1");
        assert_eq!(session.user_agent.unwrap(), "Chrome/100");
    }

    #[test]
    fn test_token_id_helper() {
        let session = SessionData::new_full(
            "anton",
            Some("jti_456".into()),
            None,
            None,
            Duration::from_secs(10),
        );

        assert_eq!(session.token_id().unwrap(), "jti_456");
    }

    #[test]
    fn test_token_expiration_check() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // токен, который истёк 10 секунд назад
        let expired_claims = TokenClaims {
            jti: "old_token".into(),
            sub: "anton".into(),
            permissions: String::new(),
            iat: now - 100,
            exp: now - 10,
            token_type: "access".into(),
        };

        let session = SessionData::new_with_token(
            "anton",
            expired_claims,
            None,
            None,
            Duration::from_secs(3600),
        );

        assert!(session.is_token_expired());

        // токен, который ещё валиден
        let valid_claims = TokenClaims {
            jti: "new_token".into(),
            sub: "anton".into(),
            permissions: String::new(),
            iat: now,
            exp: now + 3600,
            token_type: "access".into(),
        };

        let session2 = SessionData::new_with_token(
            "anton",
            valid_claims,
            None,
            None,
            Duration::from_secs(3600),
        );

        assert!(!session2.is_token_expired());
    }

    #[test]
    fn test_ip_validation() {
        let session = SessionData::new("anton", Some("127.0.0.1".into()), Duration::from_secs(10));

        assert!(session.validate_ip(Some("127.0.0.1")));
        assert!(!session.validate_ip(Some("192.168.1.1")));
        assert!(session.validate_ip(None));
    }

    #[test]
    fn test_user_agent_validation() {
        let session = SessionData::new_full(
            "anton",
            None,
            None,
            Some("Chrome/100".into()),
            Duration::from_secs(10),
        );

        assert!(session.validate_user_agent(Some("Chrome/100")));
        assert!(!session.validate_user_agent(Some("Firefox/90")));
        assert!(session.validate_user_agent(None));
    }

    #[test]
    fn test_fingerprint_validation() {
        let claims = TokenClaims {
            jti: "tok1".into(),
            sub: "anton".into(),
            permissions: "+@all".into(),
            iat: 1000,
            exp: 9999,
            token_type: "access".into(),
        };

        let session = SessionData::new_with_token(
            "anton",
            claims,
            Some("127.0.0.1".into()),
            Some("Chrome/100".into()),
            Duration::from_secs(10),
        );

        assert!(session.validate_fingerprint(Some("127.0.0.1"), Some("Chrome/100")));
        assert!(!session.validate_fingerprint(Some("127.0.0.1"), Some("Firefox/90")));
        assert!(!session.validate_fingerprint(Some("192.168.1.1"), Some("Chrome/100")));
    }

    #[test]
    fn test_permissions_helper() {
        let claims = TokenClaims {
            jti: "t1".into(),
            sub: "anton".into(),
            permissions: "+@read,+@write,-del".into(),
            iat: 1000,
            exp: 2000,
            token_type: "access".into(),
        };

        let session =
            SessionData::new_with_token("anton", claims, None, None, Duration::from_secs(10));

        assert_eq!(session.permissions().unwrap(), "+@read,+@write,-del");
    }

    #[test]
    fn test_serde_round_trip() {
        let claims = TokenClaims {
            jti: "test_jti".into(),
            sub: "test_user".into(),
            permissions: "+@all".into(),
            iat: 1000,
            exp: 2000,
            token_type: "access".into(),
        };

        let session = SessionData::new_with_token(
            "test_user",
            claims,
            Some("127.0.0.1".into()),
            Some("TestAgent".into()),
            Duration::from_secs(3600),
        );

        // сериализация в JSON
        let json = serde_json::to_string(&session).unwrap();

        // десериализация в JSON
        let restored: SessionData = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.username, "test_user");
        assert_eq!(restored.token_id().unwrap(), "test_jti");
        assert_eq!(restored.ip_address.unwrap(), "127.0.0.1");
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
