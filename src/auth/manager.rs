// Copyright 2025 Zumic

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

use crate::{AclError, AuthError, PasswordError};

use super::{lookup_cmd_idx, parse_category};
use super::{
    Acl, ServerConfig, {hash_password, verify_password},
};

/// Максимальное количество неудачных попыток входа перед временной блокировкой.
const MAX_FAILS: u8 = 5;
/// Длительность блокировки после превышения `MAX_FAILS`.
const BLOCK_DURATION: Duration = Duration::from_secs(60);

/// Менеджер аутентификации и авторизации пользователей.
///
/// Хранит ACL, опциональную «pepper»-строку для хеширования паролей и
/// информацию о неудачных попытках входа (для rate-limiting).
#[derive(Debug)]
pub struct AuthManager {
    /// Ссылка на ACL-систему для проверки прав доступа.
    acl: Arc<RwLock<Acl>>,
    /// Опциональная «pepper»-строка, добавляемая к паролям перед хешированием.
    pepper: Option<String>,
    /// Счётчик неудачных попыток входа: имя пользователя → (кол-во, время первой неудачи).
    failures: Arc<RwLock<HashMap<String, (u8, Instant)>>>,
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::new() // Using the existing `new()` method as the default constructor
    }
}

impl AuthManager {
    /// Создаёт нового `AuthManager` без «pepper».
    pub fn new() -> Self {
        Self {
            acl: Arc::new(RwLock::new(Acl::default())),
            pepper: None,
            failures: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Создаёт новый `AuthManager` с заданной «pepper»-строкой для хеширования.
    pub fn with_pepper(pepper: impl Into<String>) -> Self {
        Self {
            acl: Arc::new(RwLock::new(Acl::default())),
            pepper: Some(pepper.into()),
            failures: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Создаёт пользователя с паролем и набором ACL-правил.
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

    /// Аутентифицирует пользователя по имени и паролю.
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

        // Получаем шаблоны паролей из ACL
        let acl = self.acl.read().await;
        let user = acl.acl_getuser(username).ok_or(AuthError::UserNotFound)?;
        let pepper = self.pepper.clone();
        let hashes = user.password_hashes.clone();
        let password = password.to_owned();

        // Если пустой список хешей - пароль не требуется (nopass)
        let ok: bool = if hashes.is_empty() {
            true
        } else {
            tokio::task::spawn_blocking(move || {
                hashes.iter().any(|hash| {
                    verify_password(hash, &password, pepper.as_deref()).unwrap_or(false)
                })
            })
            .await
            .map_err(|_| AuthError::Password(PasswordError::Verify))?
        };

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

    /// Проверяет, разрешена ли пользователю команда в заданной категории.
    pub async fn authorize_command(
        &self,
        username: &str,
        category: &str,
        command: &str,
    ) -> Result<(), AuthError> {
        // Читать из RwLock нечасто, это не «горячий» путь.
        let acl = self.acl.read().await;
        let user = acl.acl_getuser(username).ok_or(AuthError::UserNotFound)?;

        // Подготовка вне горячего пути:
        let cat = parse_category(category);
        let cmd_idx = lookup_cmd_idx(command);

        // Горячий путь: одно сравнение enum/usize и битов
        if user.check_idx(cat, cmd_idx) {
            Ok(())
        } else {
            Err(AclError::PermissionDenied.into())
        }
    }

    /// Проверяет доступ пользователя к конкретному ключу.
    pub async fn authorize_key(&self, username: &str, key: &str) -> Result<(), AuthError> {
        let acl = self.acl.read().await;
        let user = acl.acl_getuser(username).ok_or(AuthError::UserNotFound)?;
        if user.check_key(key) {
            Ok(())
        } else {
            Err(AclError::PermissionDenied.into())
        }
    }

    /// Инициализирует `AuthManager` из конфигурации сервера.
    pub async fn from_config(config: &ServerConfig) -> Result<Self, AuthError> {
        let pepper = config.auth_pepper.clone();
        let acl = Acl::default();

        // Глобальный пароль
        if let Some(pass) = &config.requirepass {
            let hash = hash_password(pass, pepper.as_deref())?;
            let rules = [format!(">{hash}"), "on".into(), "~*".into(), "+@all".into()];
            let refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();

            if acl.acl_getuser("default").is_some() {
                // Если пользователь существует, обновляем его настройки
                acl.acl_setuser("default", &refs)?;
            } else {
                // Если пользователь не существует, создаём его
                acl.acl_setuser("default", &refs)?;
            }
        }

        // Пользователи из конфига
        for user_config in &config.users {
            let mut rules = Vec::new();

            if !user_config.nopass {
                if let Some(pass) = &user_config.password {
                    let hash = hash_password(pass, pepper.as_deref())?;
                    rules.push(format!(">{hash}"));
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

    /// Возвращает клонированный `Arc<RwLock<Acl>>`, чтобы можно было
    /// проверить или изменить ACL извне.
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
    use tokio;

    use super::*;

    /// Тест проверяет создание пользователя и успешную/неуспешную аутентификацию.
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

    /// Тест проверяет доступ пользователя к командам по категориям и индивидуальным разрешениям.
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

    /// Тест проверяет, что работает ограничение доступа к ключам по шаблону.
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

    /// Тест проверяет, что при множестве неудачных попыток входа срабатывает rate-limiting.
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

    #[tokio::test]
    async fn test_create_user_duplicate() {
        let m = AuthManager::new();
        m.create_user("anton", "pw", &[]).await.unwrap();
        let err = m.create_user("anton", "pw", &[]).await.unwrap_err();
        assert!(matches!(err, AuthError::UserAlreadyExists));
    }

    #[tokio::test]
    async fn test_pepper_changes_hash() {
        let m1 = AuthManager::new();
        let m2 = AuthManager::with_pepper("pep");
        m1.create_user("anton", "secret", &[]).await.unwrap();
        m2.create_user("anton2", "secret", &[]).await.unwrap();

        // Хеши разные, поэтому из m1 не залогиниться через м2 и наоборот
        assert!(m1.authenticate("anton", "secret").await.is_ok());
        assert!(matches!(
            m2.authenticate("anton", "secret").await,
            Err(AuthError::UserNotFound)
        ));
    }

    #[tokio::test]
    async fn test_block_expires() {
        tokio::time::pause();
        let manager = AuthManager::new();
        manager.create_user("u", "p", &[]).await.unwrap();

        for _ in 0..MAX_FAILS {
            let _ = manager.authenticate("u", "wrong").await;
        }
        // сейчас заблокирован
        assert!(matches!(
            manager.authenticate("u", "p").await.unwrap_err(),
            AuthError::TooManyAttempts
        ));

        // двигаем время вперёд на BLOCK_DURATION
        tokio::time::advance(BLOCK_DURATION).await;
        // блокировка сброшена
        assert!(manager.authenticate("u", "p").await.is_ok());
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let m1 = AuthManager::new();
        m1.create_user("u", "p", &[]).await.unwrap();
        let m2 = m1.clone();

        // несколько неудачных попыток через m1
        for _ in 0..MAX_FAILS {
            let _ = m1.authenticate("u", "wrong").await;
        }
        // m2 тоже видит блокировку
        assert!(matches!(
            m2.authenticate("u", "p").await.unwrap_err(),
            AuthError::TooManyAttempts
        ));
    }
}
