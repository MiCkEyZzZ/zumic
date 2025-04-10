use std::sync::Arc;

use super::{
    acl::Acl,
    config::ServerConfig,
    errors::{AclError, AuthError},
    password::{hash_password, verify_password},
};

#[derive(Debug, Clone)]
pub struct AuthManager {
    acl: Arc<Acl>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self {
            acl: Arc::new(Acl::default()),
        }
    }

    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        permissions: &[&str],
    ) -> Result<(), AuthError> {
        if self.acl.acl_getuser(username).is_some() {
            return Err(AuthError::UserAlreadyExists);
        }

        let hash = hash_password(password)?;
        let mut rules: Vec<String> = vec![format!(">{}", hash), "on".to_string()];
        rules.extend(permissions.iter().map(|s| s.to_string()));

        let rules_ref: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
        self.acl.acl_setuser(username, &rules_ref)?;
        Ok(())
    }

    pub fn authenticate(&self, username: &str, password: &str) -> Result<(), AuthError> {
        let user = self
            .acl
            .acl_getuser(username)
            .ok_or(AuthError::UserNotFound)?;

        if verify_password(&user.password_hash.unwrap_or_default(), password)? {
            Ok(())
        } else {
            Err(AuthError::AuthenticationFailed)
        }
    }

    pub fn authorize_command(
        &self,
        username: &str,
        category: &str,
        command: &str,
    ) -> Result<(), AuthError> {
        let user = self
            .acl
            .acl_getuser(username)
            .ok_or(AuthError::UserNotFound)?;

        if user.check_permission(category, command) {
            Ok(())
        } else {
            Err(AclError::PermissionDenied.into())
        }
    }

    pub fn authorize_key(&self, username: &str, key: &str) -> Result<(), AuthError> {
        let user = self
            .acl
            .acl_getuser(username)
            .ok_or(AuthError::UserNotFound)?;

        if user.check_key(key) {
            Ok(())
        } else {
            Err(AclError::PermissionDenied.into())
        }
    }

    pub fn from_config(config: &ServerConfig) -> Result<Self, AuthError> {
        let acl = Arc::new(Acl::default());

        // requirepass → пользователь "default"
        if let Some(pass) = &config.requirepass {
            let hash = hash_password(pass)?;
            let rules: Vec<String> = vec![
                format!(">{}", hash),
                "on".into(),
                "~*".into(),
                "+@all".into(),
            ];
            let rule_refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
            acl.acl_setuser("default", &rule_refs)?;
        }

        // Доп. пользователи
        for user_config in &config.users {
            let mut rules: Vec<String> = Vec::new();

            if !user_config.nopass {
                if let Some(pass) = &user_config.password {
                    // Хэшируем пароль, как и для пользователя "default"
                    let hash = hash_password(pass)?;
                    rules.push(format!(">{}", hash));
                }
            }

            rules.push(if user_config.enabled {
                "on".to_string()
            } else {
                "off".to_string()
            });

            if user_config.nopass {
                rules.push("nopass".to_string());
            }

            rules.extend(user_config.keys.iter().cloned());
            rules.extend(user_config.permissions.iter().cloned());

            let rule_refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
            acl.acl_setuser(&user_config.username, &rule_refs)?;
        }

        Ok(Self { acl })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::auth::config::UserConfig;

    // Тест создания пользователя и успешной аутентификации
    #[test]
    fn test_create_and_authenticate_user() {
        let auth_manager = AuthManager::new();

        // Создаем пользователя с правами "+get" и "+set"
        auth_manager
            .create_user("bob", "s3cr3t", &["+get", "+set"])
            .expect("User creation should succeed");

        // Успешная аутентификация с корректным паролем
        assert!(auth_manager.authenticate("bob", "s3cr3t").is_ok());

        // Аутентификация с неправильным паролем должна вернуть ошибку
        assert!(matches!(
            auth_manager.authenticate("bob", "wrongpass"),
            Err(AuthError::AuthenticationFailed)
        ));

        // Попытка аутентификации несуществующего пользователя
        assert!(matches!(
            auth_manager.authenticate("nonexistent", "any"),
            Err(AuthError::UserNotFound)
        ));
    }

    // Тест авторизации команд для пользователя
    #[test]
    fn test_authorize_command() {
        let auth_manager = AuthManager::new();

        // Создаем пользователя с правами "+@admin" и правом на команду "set" в категории "write"
        auth_manager
            .create_user("alice", "topsecret", &["+@admin", "+@write|set"])
            .expect("User creation should succeed");

        // Авторизация существующей команды должна пройти успешно
        assert!(auth_manager
            .authorize_command("alice", "write", "set")
            .is_ok());

        // Попытка авторизации несуществующей команды должна вернуть ошибку
        assert!(auth_manager
            .authorize_command("alice", "write", "get")
            .is_err());
    }

    // Тест авторизации доступа к ключам
    #[test]
    fn test_authorize_key() {
        let auth_manager = AuthManager::new();

        // Создаем пользователя с доступом к ключам, начинающимся с "data:"
        auth_manager
            .create_user("charlie", "pass123", &["~data:*"])
            .expect("User creation should succeed");

        // Ключ, удовлетворяющий шаблону, должен авторизоваться
        assert!(auth_manager.authorize_key("charlie", "data:123").is_ok());

        // Ключ, не удовлетворяющий шаблону, должен вернуть ошибку
        assert!(auth_manager.authorize_key("charlie", "info:123").is_err());
    }

    // Тест инициализации через конфигурацию (from_config)
    #[test]
    fn test_from_config() {
        // Создаем конфигурацию с requirepass для пользователя "default"
        // и дополнительного пользователя "dave"
        let mut config = ServerConfig::default();
        config.requirepass = Some("foobared".to_string());
        config.users.push(UserConfig {
            username: "dave".to_string(),
            enabled: true,
            nopass: false,
            password: Some("davepassword".to_string()),
            keys: vec!["~davekey".to_string()],
            permissions: vec!["+@custom".to_string()],
        });

        let auth_manager = AuthManager::from_config(&config).expect("Config should be parsed");

        // Проверяем, что создан пользователь "default" с requirepass
        assert!(auth_manager.authenticate("default", "foobared").is_ok());
        // Для пользователя default, согласно конфигурации, доступ к ключам не ограничен,
        // поэтому authorize_key всегда должен возвращать Ok.
        assert!(auth_manager.authorize_key("default", "any_key").is_ok());

        // Проверяем пользователя "dave"
        assert!(auth_manager.authenticate("dave", "davepassword").is_ok());
        // Доступ к ключу, удовлетворяющему шаблону "~davekey"
        assert!(auth_manager.authorize_key("dave", "davekey").is_ok());
        // Попытка авторизации для ключа, не соответствующего шаблону, должна вернуть ошибку
        assert!(auth_manager.authorize_key("dave", "otherkey").is_err());
    }
}
