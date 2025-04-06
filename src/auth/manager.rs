use std::sync::Arc;
use thiserror::Error;

use super::{
    acl::{Acl, AclError},
    config::ServerConfig,
    password::{hash_password, verify_password, PasswordError},
};

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Authentication failed")]
    AuthenticationFailed,
    #[error("User not found")]
    UserNotFound,
    #[error("Password error: {0}")]
    Password(#[from] PasswordError),
    #[error("ACL error: {0}")]
    Acl(#[from] AclError),
}

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
                    rules.push(format!(">{}", pass));
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
