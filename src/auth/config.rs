use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Default)]
pub struct ServerConfig {
    pub requirepass: Option<String>,
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

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
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
