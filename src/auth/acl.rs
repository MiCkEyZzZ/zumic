use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AclError {
    #[error("User already exists")]
    UserExists,
    #[error("User not found")]
    UserNotFound,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Authentication failed")]
    AuthFailed,
    #[error("Channel access denied")]
    ChannelDenied,
}

#[derive(Debug, Clone)]
pub struct AclUser {
    pub username: String,
    pub password_hash: Option<String>,
    pub enabled: bool,
    pub permissions: HashSet<String>,
    pub channels: HashSet<String>,
    pub keys: HashSet<String>, // Шаблоны ключей, к которым есть доступ
}

#[derive(Debug, Default)]
pub struct Acl {
    users: RwLock<HashMap<String, Arc<RwLock<AclUser>>>>,
}

impl AclUser {
    pub fn new(username: &str) -> Self {
        Self {
            username: username.to_string(),
            password_hash: None,
            enabled: true,
            permissions: ["+@all"].iter().map(|s| s.to_string()).collect(),
            channels: ["*"].iter().map(|s| s.to_string()).collect(),
            keys: ["*"].iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn authenticate(&self, password_hash: &str) -> Result<(), AclError> {
        if !self.enabled {
            return Err(AclError::AuthFailed);
        }
        match &self.password_hash {
            Some(hash) if hash == password_hash => Ok(()),
            _ => Err(AclError::AuthFailed),
        }
    }

    pub fn check_permission(&self, category: &str, command: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let perm = format!("+@{category}|{command}");
        self.permissions.contains("*")
            || self.permissions.contains(&format!("+@{category}"))
            || self.permissions.contains(&perm)
    }

    pub fn check_channel(&self, channel: &str) -> bool {
        self.enabled
            && (self.channels.contains("*") && !self.channels.contains(channel) == false
                || self.channels.contains(channel))
    }

    pub fn check_key(&self, key: &str) -> bool {
        if !self.enabled {
            return false;
        }

        if self.keys.contains("*") {
            return true;
        }

        self.keys
            .iter()
            .any(|pattern| pattern == key || key_matches(pattern, key))
    }
}

fn key_matches(pattern: &str, key: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let key: Vec<char> = key.chars().collect();
    let mut p_idx = 0;
    let mut k_idx = 0;
    let mut backtrack: Option<(usize, usize)> = None;

    while k_idx < key.len() {
        if p_idx < pattern.len() {
            match pattern[p_idx] {
                '*' => {
                    backtrack = Some((p_idx + 1, k_idx));
                    p_idx += 1;
                    continue;
                }
                '?' => {
                    k_idx += 1;
                    p_idx += 1;
                    continue;
                }
                pc => {
                    if k_idx < key.len() && key[k_idx] == pc {
                        k_idx += 1;
                        p_idx += 1;
                        continue;
                    }
                }
            }
        }

        if let Some((bp, bk)) = backtrack {
            if bk < key.len() {
                p_idx = bp;
                k_idx = bk + 1;
                backtrack = Some((bp, bk + 1));
                continue;
            }
        }

        return false;
    }

    // Проверяем, что в шаблоне не осталось символов, кроме '*'
    while p_idx < pattern.len() && pattern[p_idx] == '*' {
        p_idx += 1;
    }

    p_idx == pattern.len()
}

impl Acl {
    pub fn acl_setuser(&self, username: &str, rules: &[&str]) -> Result<(), AclError> {
        let mut users = self.users.write().unwrap();
        let user = users
            .entry(username.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(AclUser::new(username))));

        let mut user = user.write().unwrap();
        for rule in rules {
            self.apply_rule(&mut user, rule)?;
        }
        Ok(())
    }

    fn apply_rule(&self, user: &mut AclUser, rule: &str) -> Result<(), AclError> {
        match rule {
            "on" => user.enabled = true,
            "off" => user.enabled = false,
            _ if rule.starts_with('>') => {
                user.password_hash = Some(rule[1..].to_string());
            }
            _ if rule.starts_with('+') || rule.starts_with('-') => {
                user.permissions.insert(rule.to_string());
            }
            _ if rule.starts_with('~') => {
                // Если присутствует глобальный доступ "*", удаляем его перед установкой конкретного шаблона
                if user.keys.contains("*") {
                    user.keys.clear();
                }
                user.keys.insert(rule[1..].to_string());
            }
            _ if rule.starts_with('&') => {
                user.channels.insert(rule[1..].to_string());
            }
            _ => return Err(AclError::PermissionDenied),
        }
        Ok(())
    }

    pub fn acl_getuser(&self, username: &str) -> Option<AclUser> {
        self.users
            .read()
            .unwrap()
            .get(username)
            .map(|u| u.read().unwrap().clone())
    }

    pub fn acl_deluser(&self, username: &str) -> Result<(), AclError> {
        self.users
            .write()
            .unwrap()
            .remove(username)
            .map(|_| ())
            .ok_or(AclError::UserNotFound)
    }

    pub fn acl_users(&self) -> Vec<String> {
        self.users.read().unwrap().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Утверждаем, что создаём пользователя и он добавляется в систему
    #[test]
    fn test_acl_setuser() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "+@admin", "~key1", "&channel1"];

        assert!(acl.acl_setuser(username, &rules).is_ok());
        let user = acl.acl_getuser(username).unwrap();

        assert_eq!(user.username, username);
        assert_eq!(user.password_hash.unwrap(), "password123");
        assert!(user.permissions.contains("+@admin"));
        assert!(user.keys.contains("key1"));
        assert!(user.channels.contains("channel1"));
    }

    // Проверяем, что если пользователя нет, то он не может аутентифицироваться
    #[test]
    fn test_authenticate_user_no_found() {
        let acl = Acl::default();
        let username = "non_existent_user";
        assert!(acl.acl_getuser(username).is_none());
    }

    // Проверка на успешную аутентификацию
    #[test]
    fn test_authenticate_success() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "+@admin"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(user.authenticate("password123").is_ok());
    }

    // Проверка на неудачную аутентификацию с неправильным паролем
    #[test]
    fn test_authenticate_failure() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "+@admin"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(user.authenticate("wrong_password").is_err());
    }

    // Проверка разрешения на команду с нужной категорией
    #[test]
    fn test_check_permission_success() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "+@admin", "+@write|set"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(user.check_permission("write", "set"));
    }

    // Проверка отказа в разрешении на команду
    #[test]
    fn test_check_permission_failure() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "+@admin"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(!user.check_permission("write", "set"));
    }

    // Проверка доступа к каналу
    #[test]
    fn test_check_channel_access() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "&channel1"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(user.check_channel("channel1"));
    }

    // Проверка отказа в доступе к каналу
    #[test]
    fn test_check_channel_denied() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "&channel1"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(!user.check_channel("channel2"));
    }

    // Проверка доступа к ключу
    #[test]
    fn test_check_key_access() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "~key1"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(user.check_key("key1"));
    }

    // Проверка отказа в доступе к ключу.
    #[test]
    fn test_check_key_denied() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "~key1"];
        acl.acl_setuser(username, &rules).unwrap();

        let user = acl.acl_getuser(username).unwrap();
        assert!(
            !user.check_key("key2"),
            "User should not have access to key2"
        );
        assert!(
            !user.check_key("key1_extra"),
            "User should not have access to similar keys"
        );
        assert!(
            !user.check_key("key"),
            "User should not have access to partial matches"
        );
    }

    // Удаление пользователя
    #[test]
    fn test_acl_deluser() {
        let acl = Acl::default();
        let username = "test_user";
        let rules = ["on", ">password123", "+@admin"];
        acl.acl_setuser(username, &rules).unwrap();

        assert!(acl.acl_deluser(username).is_ok());
        assert!(acl.acl_getuser(username).is_none());
    }

    // Проверка удаления несуществующего пользователя
    #[test]
    fn test_acl_deluser_not_found() {
        let acl = Acl::default();
        assert!(acl.acl_deluser("non_existent_user").is_err());
    }
}
