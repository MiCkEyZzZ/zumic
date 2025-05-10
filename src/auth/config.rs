// Copyright 2025 Zumic

use std::fs;
use std::path::Path;

use crate::ConfigError;

#[derive(Debug, Default)]
pub struct ServerConfig {
    pub requirepass: Option<String>,
    pub auth_pepper: Option<String>,
    pub users: Vec<UserConfig>,
}

#[derive(Debug)]
pub struct UserConfig {
    pub username: String,
    pub enabled: bool,
    pub nopass: bool,
    pub password: Option<String>,
    pub keys: Vec<String>,
    pub permissions: Vec<String>,
}

impl ServerConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)?;
        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        let mut config = ServerConfig::default();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(pass) = line.strip_prefix("requirepass ") {
                config.requirepass = Some(pass.trim().to_string());
            } else if let Some(pepper) = line.strip_prefix("auth-pepper ") {
                config.auth_pepper = Some(pepper.trim().to_string());
            } else if let Some(user_line) = line.strip_prefix("user ") {
                let user = Self::parse_user(user_line)?;
                config.users.push(user);
            }
        }

        Ok(config)
    }

    fn parse_user(line: &str) -> Result<UserConfig, ConfigError> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(ConfigError::Parse("Invalid user format".into()));
        }

        let mut user = UserConfig {
            username: parts[0].to_string(),
            enabled: false,
            nopass: false,
            password: None,
            keys: Vec::new(),
            permissions: Vec::new(),
        };

        for part in &parts[1..] {
            match *part {
                "on" => user.enabled = true,
                "off" => user.enabled = false,
                "nopass" => user.nopass = true,
                _ if part.starts_with('~') => user.keys.push(part.to_string()),
                _ if part.starts_with('>') => {
                    user.password = Some(part[1..].to_string());
                }
                _ if part.starts_with('+') || part.starts_with('-') => {
                    user.permissions.push(part.to_string());
                }
                _ => {
                    return Err(ConfigError::Parse(format!(
                        "Unknown user directive: {}",
                        part
                    )))
                }
            }
        }

        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет, что директива `requirepass` корректно парсится
    /// и сохраняется в поле `requirepass`, а список пользователей остаётся пустым.
    #[test]
    fn test_parse_requirepass() {
        let content = "requirepass kinzaza";
        let config = ServerConfig::parse(content).unwrap();
        assert_eq!(config.requirepass.unwrap(), "kinzaza");
        assert!(config.users.is_empty());
    }

    /// Тест проверяет корректность парсинга одного пользователя с параметрами:
    /// включён (`on`), без пароля (`nopass`), с доступом ко всем ключам (`~*`) и всеми правами (`+@all`).
    #[test]
    fn test_parse_single_user() {
        let content = "user default on nopass ~* +@all";
        let config = ServerConfig::parse(content).unwrap();
        assert_eq!(config.users.len(), 1);
        let user = &config.users[0];
        assert_eq!(user.username, "default");
        assert!(user.enabled);
        assert!(user.nopass);
        // Проверяем, что директива ключей присутствует в виде строки "~*"
        assert!(user.keys.contains(&"~*".to_string()));
        // Проверяем, что директива прав присутствует
        assert!(user.permissions.contains(&"+@all".to_string()));
    }

    /// Тест проверяет парсинг конфигурации с несколькими пользователями.
    /// Удостоверяется, что `requirepass` установлен и оба пользователя корректно разобраны.
    #[test]
    fn test_parse_multiple_users() {
        let content = "\
    requirepass kinzaza
    user default on nopass ~* +@all
    user anton on >supersecret ~data:* +get +set";
        let config = ServerConfig::parse(content).unwrap();
        assert_eq!(config.requirepass.unwrap(), "kinzaza");
        assert_eq!(config.users.len(), 2);

        let anton = &config.users[1];
        assert_eq!(anton.username, "anton");
        assert!(anton.enabled);
        // Здесь пароль хранится без префикса '>', т.е. просто "supersecret"
        assert_eq!(anton.password.as_ref().unwrap(), "supersecret");
        assert!(anton.keys.contains(&"~data:*".to_string()));
        assert!(anton.permissions.contains(&"+get".to_string()));
        assert!(anton.permissions.contains(&"+set".to_string()));
    }

    /// Тест проверяет поведение при попытке парсинга пользователя в некорректном формате:
    /// ожидание ошибки из-за недостаточного количества параметров.
    #[test]
    fn test_parse_invalid_user_format() {
        // Должно вернуть ошибку, так как формат пользователя неверный (меньше 3-х частей)
        let content = "user default";
        let result = ServerConfig::parse(content);
        assert!(result.is_err());
    }

    /// Тест проверяет, что при наличии неизвестной директивы в строке пользователя возникает ошибка парсинга.
    #[test]
    fn test_unknown_directive() {
        // Если встретилась неизвестная директива, должен возникнуть ParseError
        let content = "user default on unknown_directive";
        let result = ServerConfig::parse(content);
        assert!(result.is_err());
    }
}
