use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use hmac::{digest::KeyInit, Hmac};
use jwt::{SignWithKey, VerifyWithKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::sync::RwLock;
use uuid::Uuid;
use zumic_error::{AuthError, StackError, ZumicResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenClaims {
    pub jti: String,
    pub sub: String,
    pub permissions: String,
    pub iat: u64,
    pub exp: u64,
    pub token_type: String,
}

#[derive(Debug, Clone)]
pub struct TokenConfig {
    pub access_token_ttl: u64,
    pub refresh_token_ttl: u64,
    pub secret_key: String,
    pub auto_cleanup: bool,
    pub cleanup_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Clone)]
pub struct TokenManager {
    config: Arc<RwLock<TokenConfig>>,
    revoked_tokens: Arc<DashMap<String, u64>>,
    hmac_key: Arc<RwLock<Hmac<Sha256>>>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl TokenConfig {
    pub fn with_secret(secret: impl Into<String>) -> Self {
        Self {
            secret_key: secret.into(),
            ..Default::default()
        }
    }

    pub fn access_ttl(
        mut self,
        ttl: u64,
    ) -> Self {
        self.access_token_ttl = ttl;
        self
    }

    pub fn refresh_ttl(
        mut self,
        ttl: u64,
    ) -> Self {
        self.refresh_token_ttl = ttl;
        self
    }

    fn generate_secret() -> String {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
        base64::encode(&bytes)
    }
}

impl TokenManager {
    pub fn new(config: TokenConfig) -> ZumicResult<Self> {
        let key = Hmac::<Sha256>::new_from_slice(config.secret_key.as_bytes()).map_err(|_| {
            AuthError::InvalidKey {
                reason: "asd".to_string(),
            }
        })?;

        let manager = Self {
            config: Arc::new(RwLock::new(config.clone())),
            revoked_tokens: Arc::new(DashMap::new()),
            hmac_key: Arc::new(RwLock::new(key)),
        };

        if config.auto_cleanup {
            let mgr = manager.clone();
            tokio::spawn(async move {
                mgr.cleanup_loop().await;
            });
        }

        Ok(manager)
    }

    pub async fn generate_token_pair(
        &self,
        username: &str,
        permissions: &[&str],
    ) -> ZumicResult<TokenPair> {
        let config = self.config.read().await;
        let now = Self::current_timestamp();

        let access_jti = Uuid::new_v4().to_string();
        let access_claims = TokenClaims {
            jti: access_jti,
            sub: username.to_string(),
            permissions: permissions.join(","),
            iat: now,
            exp: now + config.access_token_ttl,
            token_type: "access".to_string(),
        };

        let refresh_jti = Uuid::new_v4().to_string();
        let refresh_claims = TokenClaims {
            jti: refresh_jti,
            sub: username.to_string(),
            permissions: permissions.join(","),
            iat: now,
            exp: now + config.refresh_token_ttl,
            token_type: "refresh".to_string(),
        };

        let key = self.hmac_key.read().await;
        let access_token =
            access_claims
                .sign_with_key(&*key)
                .map_err(|_| AuthError::SigningFailed {
                    reason: "sdsd".to_string(),
                })?;
        let refresh_token =
            refresh_claims
                .sign_with_key(&*key)
                .map_err(|_| AuthError::SigningFailed {
                    reason: "sdsd".to_string(),
                })?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: config.access_token_ttl,
        })
    }

    pub async fn verify_token(
        &self,
        token: &str,
    ) -> ZumicResult<TokenClaims> {
        let key = self.hmac_key.read().await;
        let claims: TokenClaims =
            token
                .verify_with_key(&*key)
                .map_err(|_| AuthError::InvalidToken {
                    reason: "sd".to_string(),
                })?;

        let now = Self::current_timestamp();
        if claims.exp < now {
            return Err(AuthError::InvalidToken {
                reason: "asd".to_string(),
            })?;
        }

        if self.is_revoked(&claims.jti) {
            return Err(StackError::from(AuthError::Revoked {
                reason: "df".to_string(),
            }));
        }

        Ok(claims)
    }

    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> ZumicResult<TokenPair> {
        let claims = self.verify_token(refresh_token).await?;

        if claims.token_type != "refresh" {
            return Err(StackError::from(AuthError::InvalidToken {
                reason: "sd".to_string(),
            }));
        }

        let permissions: Vec<&str> = claims.permissions.split(',').collect();
        self.generate_token_pair(&claims.sub, &permissions).await
    }

    pub fn revoke_token(
        &self,
        jti: &str,
        exp: u64,
    ) {
        self.revoked_tokens.insert(jti.to_string(), exp);
    }

    pub async fn revoke_token_claims(
        &self,
        token: &str,
    ) -> ZumicResult<()> {
        let claims = self.verify_token(token).await?;
        self.revoke_token(&claims.jti, claims.exp);
        Ok(())
    }

    pub fn is_revoked(
        &self,
        jti: &str,
    ) -> bool {
        self.revoked_tokens.contains_key(jti)
    }

    pub fn revoked_count(&self) -> usize {
        self.revoked_tokens.len()
    }

    pub async fn introspect(
        &self,
        token: &str,
    ) -> ZumicResult<TokenClaims> {
        self.verify_token(token).await
    }

    pub async fn cleanup_expired(&self) -> usize {
        let now = Self::current_timestamp();
        let mut removed = 0;

        self.revoked_tokens.retain(|_, exp| {
            let keep = *exp > now;
            if !keep {
                removed += 1;
            }
            keep
        });
        removed
    }

    async fn cleanup_loop(&self) {
        let interval = {
            let config = self.config.read().await;
            Duration::from_secs(config.cleanup_interval)
        };

        loop {
            tokio::time::sleep(interval).await;
            let removed = self.cleanup_expired().await;
            if removed > 0 {
                tracing::debug!("Cleaned up {} expired revoked tokens", removed);
            }
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для TokenConfig
////////////////////////////////////////////////////////////////////////////////

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            access_token_ttl: 15 * 60,
            refresh_token_ttl: 7 * 24 * 3600,
            secret_key: Self::generate_secret(),
            auto_cleanup: true,
            cleanup_interval: 3600,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_manager() -> TokenManager {
        let config = TokenConfig::default().access_ttl(60).refresh_ttl(3600);
        TokenManager::new(config).unwrap()
    }

    #[tokio::test]
    async fn test_generate_and_verify_token() {
        let manager = setup_manager().await;
        let pair = manager
            .generate_token_pair("cry", &["+@read", "+get"])
            .await
            .unwrap();

        let claims = manager.verify_token(&pair.access_token).await.unwrap();
        assert_eq!(claims.sub, "cry");
        assert_eq!(claims.permissions, "+@read,+get");
        assert_eq!(claims.token_type, "access");

        let refresh_claims = manager.verify_token(&pair.refresh_token).await.unwrap();
        assert_eq!(refresh_claims.sub, "cry");
        assert_eq!(refresh_claims.token_type, "refresh");
    }

    #[tokio::test]
    async fn test_refresh_token_flow() {
        let manager = setup_manager().await;
        let pair = manager
            .generate_token_pair("stepan", &["+@admin"])
            .await
            .unwrap();

        // Используем refresh токен, чтобы получить новый токен доступа
        let new_pair = manager
            .refresh_access_token(&pair.refresh_token)
            .await
            .unwrap();

        // Новый токен доступа должен быть действительным
        let claims = manager.verify_token(&new_pair.access_token).await.unwrap();
        assert_eq!(claims.sub, "stepan");
        assert_eq!(claims.permissions, "+@admin");
    }

    #[tokio::test]
    async fn test_token_introspection() {
        let manager = setup_manager().await;
        let pair = manager
            .generate_token_pair("pavel", &["+@read", "+set"])
            .await
            .unwrap();

        let claims = manager.introspect(&pair.access_token).await.unwrap();
        assert_eq!(claims.sub, "pavel");
        assert_eq!(claims.permissions, "+@read,+set");
        assert!(claims.exp > claims.iat);
    }
}
