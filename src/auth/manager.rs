use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;

use super::{
    acl::Acl,
    config::ServerConfig,
    errors::{AclError, AuthError, PasswordError},
    password::{hash_password, verify_password},
};

const MAX_FAILS: u8 = 5;
const BLOCK_DURATION: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct AuthManager {
    acl: Arc<RwLock<Acl>>,
    pepper: Option<String>,
    failures: Arc<RwLock<HashMap<String, (u8, Instant)>>>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self {
            acl: Arc::new(RwLock::new(Acl::default())),
            pepper: None,
            failures: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_pepper(pepper: impl Into<String>) -> Self {
        Self {
            acl: Arc::new(RwLock::new(Acl::default())),
            pepper: Some(pepper.into()),
            failures: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        permissions: &[&str],
    ) -> Result<(), AuthError> {
        let hash = hash_password(password, self.pepper.as_deref())?;
        let mut rules: Vec<String> = vec![format!(">{}", hash), "on".into()];
        rules.extend(permissions.iter().map(|s| s.to_string()));
        let rules_ref: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();

        let acl = self.acl.write().await;
        if acl.acl_getuser(username).is_some() {
            return Err(AuthError::UserAlreadyExists);
        }

        acl.acl_setuser(username, &rules_ref)?;
        Ok(())
    }

    pub async fn authenticate(&self, username: &str, password: &str) -> Result<(), AuthError> {
        // Проверка блокировки пользователя по неудачным попыткам
        {
            let mut failures = self.failures.write().await;
            if let Some((count, ts)) = failures.get(username) {
                if *count >= MAX_FAILS && ts.elapsed() < BLOCK_DURATION {
                    return Err(AuthError::TooManyAttempts);
                } else if ts.elapsed() >= BLOCK_DURATION {
                    failures.remove(username); // сбрасываем счётчик
                }
            }
        }

        let acl = self.acl.read().await;
        let user = acl.acl_getuser(username).ok_or(AuthError::UserNotFound)?;

        let pepper = self.pepper.clone();
        let hashes = user.password_hashes.clone();
        let password = password.to_owned();

        let ok = tokio::task::spawn_blocking(move || {
            hashes
                .iter()
                .any(|hash| verify_password(hash, &password, pepper.as_deref()).unwrap_or(false))
        })
        .await
        .map_err(|_| AuthError::Password(PasswordError::Verify))?;

        if ok {
            let mut failures = self.failures.write().await;
            failures.remove(username);
            Ok(())
        } else {
            let mut failures = self.failures.write().await;
            let entry = failures
                .entry(username.to_string())
                .or_insert((0, Instant::now()));
            entry.0 += 1;
            Err(AuthError::AuthenticationFailed)
        }
    }

    pub async fn authorize_command(
        &self,
        username: &str,
        category: &str,
        command: &str,
    ) -> Result<(), AuthError> {
        let acl = self.acl.read().await;
        let user = acl.acl_getuser(username).ok_or(AuthError::UserNotFound)?;
        if user.check_permission(category, command) {
            Ok(())
        } else {
            Err(AclError::PermissionDenied.into())
        }
    }

    pub async fn authorize_key(&self, username: &str, key: &str) -> Result<(), AuthError> {
        let acl = self.acl.read().await;
        let user = acl.acl_getuser(username).ok_or(AuthError::UserNotFound)?;
        if user.check_key(key) {
            Ok(())
        } else {
            Err(AclError::PermissionDenied.into())
        }
    }

    pub async fn from_config(config: &ServerConfig) -> Result<Self, AuthError> {
        let pepper = config.auth_pepper.clone();
        let acl = Acl::default();

        if let Some(pass) = &config.requirepass {
            let hash = hash_password(pass, pepper.as_deref())?;
            let rules = vec![
                format!(">{}", hash),
                "on".into(),
                "~*".into(),
                "+@all".into(),
            ];
            let refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
            acl.acl_setuser("default", &refs)?;
        }

        for user_config in &config.users {
            let mut rules = Vec::new();

            if !user_config.nopass {
                if let Some(pass) = &user_config.password {
                    let hash = hash_password(pass, pepper.as_deref())?;
                    rules.push(format!(">{}", hash));
                }
            }

            rules.push(if user_config.enabled { "on" } else { "off" }.to_string());

            if user_config.nopass {
                rules.push("nopass".into());
            }

            rules.extend(user_config.keys.iter().cloned());
            rules.extend(user_config.permissions.iter().cloned());

            let refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
            acl.acl_setuser(&user_config.username, &refs)?;
        }

        Ok(Self {
            acl: Arc::new(RwLock::new(acl)),
            pepper,
            failures: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn acl(&self) -> Arc<RwLock<Acl>> {
        Arc::clone(&self.acl)
    }
}

impl Clone for AuthManager {
    fn clone(&self) -> Self {
        Self {
            acl: Arc::clone(&self.acl),
            pepper: self.pepper.clone(),
            failures: Arc::clone(&self.failures),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    use crate::auth::config::UserConfig;

    // Тест проверяет создание пользователя и успешную/неуспешную аутентификацию.
    #[tokio::test]
    async fn test_create_and_authenticate() {
        let manager = AuthManager::new();

        // Создание пользователя anton с паролем и разрешениями.
        manager
            .create_user("anton", "secret", &["+get", "+@read"])
            .await
            .unwrap();
        // Успешная аутентификация по правильному паролю.
        assert!(manager.authenticate("anton", "secret").await.is_ok());
        // Ошибка при попытке входа с неправильным паролем.
        let err = manager.authenticate("anton", "wrong").await.unwrap_err();
        assert!(matches!(err, AuthError::AuthenticationFailed));
        // Ошибка при попытке входа несуществующим пользователем.
        let err = manager.authenticate("nobody", "any").await.unwrap_err();
        assert!(matches!(err, AuthError::UserNotFound));
    }

    // Тест проверяет доступ пользователя к командам по категориям и индивидуальным разрешениям.
    #[tokio::test]
    async fn test_authorize_command() {
        let manager = AuthManager::new();
        manager
            .create_user("anton", "pw", &["+@write", "+get"])
            .await
            .unwrap();
        manager.authenticate("anton", "pw").await.unwrap();

        // Доступ к write-команде (разрешено через категорию).
        assert!(manager
            .authorize_command("anton", "write", "del")
            .await
            .is_ok());
        // Доступ к команде get вне категорий (разрешено явно).
        assert!(manager
            .authorize_command("anton", "any", "get")
            .await
            .is_ok());
        // Запрет на read-команду set (не разрешена ни категорией, ни индивидуально).
        let err = manager
            .authorize_command("anton", "read", "set")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::Acl(AclError::PermissionDenied)));
    }

    // Тест проверяет, что работает ограничение доступа к ключам по шаблону.
    #[tokio::test]
    async fn test_authorize_key() {
        let manager = AuthManager::new();
        // Пользователь с доступом только к ключам вида data:*
        manager
            .create_user("anton", "pw", &["~data:*"])
            .await
            .unwrap();
        manager.authenticate("anton", "pw").await.unwrap();

        // Ключ, соответствующий шаблону, разрешён.
        assert!(manager.authorize_key("anton", "data:123").await.is_ok());

        // Ключ, не попадающий под шаблон, запрещён.
        let err = manager.authorize_key("anton", "other").await.unwrap_err();
        assert!(matches!(err, AuthError::Acl(AclError::PermissionDenied)));
    }

    // Тест проверяет, что при множестве неудачных попыток входа срабатывает rate-limiting.
    #[tokio::test]
    async fn test_rate_limiting() {
        let manager = AuthManager::new();
        manager.create_user("d", "x", &[]).await.unwrap();

        // Совершаем MAX_FAILS неудачных попыток.
        for _ in 0..MAX_FAILS {
            let _ = manager.authenticate("d", "wrong").await;
        }

        // После превышения лимита должна сработать блокировка.
        let err = manager.authenticate("d", "wrong").await.unwrap_err();
        assert!(matches!(err, AuthError::TooManyAttempts));
    }

    // Тест проверяет корректную инициализацию менеджера авторизации из конфигурации.
    #[tokio::test]
    async fn test_from_config() {
        let mut cfg = ServerConfig::default();
        cfg.requirepass = Some("master".into()); // глобальный пароль
        cfg.auth_pepper = Some("pep".into()); // соль
        cfg.users.push(UserConfig {
            username: "u1".into(),
            enabled: true,
            nopass: false,
            password: Some("p1".into()),
            keys: vec!["~kin:*".into()],
            permissions: vec!["+@read".into()],
        });

        // Инициализация из конфига.
        let manager = AuthManager::from_config(&cfg).await.unwrap();

        // Проверка дефолтного пользователя: глобальный пароль даёт полный доступ.
        assert!(manager.authenticate("default", "master").await.is_ok());
        assert!(manager.authorize_key("default", "anything").await.is_ok());
        assert!(manager
            .authorize_command("default", "admin", "config")
            .await
            .is_ok());

        // Проверка u1: доступ к ключам по шаблону, разрешение только на read-команды.
        assert!(manager.authenticate("u1", "p1").await.is_ok());
        assert!(manager.authorize_key("u1", "kin:zaza").await.is_ok());
        assert!(manager.authorize_command("u1", "read", "get").await.is_ok());

        // Команды категории write должны быть запрещены.
        let err = manager
            .authorize_command("u1", "write", "set")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::Acl(AclError::PermissionDenied)));
    }
}
