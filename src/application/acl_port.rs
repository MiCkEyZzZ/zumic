//! Интерфейс (порт) для управления ACL (Access Control List).
//!
//! Этот трейт описывает операции над списком пользователей и их правилами доступа:
//! - `acl_setuser` — создать или обновить пользователя с набором ACL-правил.
//! - `acl_getuser` — получить копию конфигурации пользователя по имени.
//! - `acl_deluser` — удалить пользователя по имени.
//! - `acl_users` — получить список всех имён пользователей в ACL.

use crate::{AclError, AclUser};

pub trait AclPort {
    /// Создать или пересоздать пользователя с указанным именем и списком правил.
    fn acl_setuser(&self, username: &str, rules: &[&str]) -> Result<(), AclError>;
    /// Получить копию настроек пользователя.
    fn acl_getuser(&self, username: &str) -> Option<AclUser>;
    /// Удалить пользователя из ACL.
    fn acl_deluser(&self, username: &str) -> Result<(), AclError>;
    /// Получить список всех зарегистрированных имён пользователей.
    fn acl_users(&self) -> Vec<String>;
}
