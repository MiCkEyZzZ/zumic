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
    #[error("Invalid ACL rule")]
    InvalidAclRule(String),
    #[error("Channel access denied")]
    ChannelDenied,
    #[error("Password hashing failed")]
    HashingFailed,
}

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
    #[error("User already exists")]
    UserAlreadyExists,
    #[error("Too many tries")]
    TooManyAttempts,
}

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("Password hashing failed")]
    Hash,
    #[error("Password verification failed")]
    Verify,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
}
