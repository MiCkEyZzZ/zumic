use globset::{Glob, GlobSet, GlobSetBuilder};

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use super::errors::AclError;

bitflags::bitflags! {
    /// Битовая маска категорий команд, используемая для обозначения групп команд,
    /// например, `@read`, `@write`, `@admin`.
    #[derive(Clone, Debug)]
    pub struct CmdCategory: u32 {
        /// Команды для операций чтения.
        const READ = 1 << 0;
        /// Команды для операций записи.
        const WRITE = 1 << 1;
        /// Административные команды.
        const ADMIN = 1 << 2;
        // ... добавим категории по мере необходимости
    }
}

/// Представляет одно ACL-правило, разобранное из строки конфигурации.
#[derive(Debug)]
pub enum AclRule {
    /// Включить пользователя (`on`).
    On,
    /// Выключить пользователя (`off`).
    Off,
    /// Добавить хэш пароля (`>hash`).
    PasswordHash(String),
    /// Разрешить всю категорию (`+@read`, `+@write`, `+@admin`, `+@all`).
    AllowCategory(CmdCategory),
    /// Запретить всю категорию (`-@read`, `-@write`, `-@admin`).
    DenyCategory(CmdCategory),
    /// Разрешить конкретную команду (`+get`, `+del`).
    AllowCommand(String),
    /// Запретить конкретную команду (`-flushall`, `-mset`).
    DenyCommand(String),
    /// Добавить шаблон ключей (`~pattern`).
    AllowKeyPattern(String),
    /// Добавить шаблон каналов Pub/Sub (`&pattern`).
    DenyKeyPattern(String),
    AllowChannelPattern(String),
    DenyChannelPattern(String),
}

/// Конфигурация пользователя ACL.
#[derive(Debug, Clone)]
pub struct AclUser {
    /// Имя пользователя.
    pub username: String,
    /// Флаг, обозначающий, включён ли пользователь.
    pub enabled: bool,
    /// Список хэшей паролей для поддержки ротации.
    pub password_hashes: Vec<String>,
    /// Разрешённые категории команд.
    pub allowed_categories: CmdCategory,
    /// Разрешённые конкретные команды.
    pub allowed_commands: HashSet<String>,
    /// Запрещённые конкретные команды (например, `-flushall`).
    pub denied_commands: HashSet<String>,
    /// "Сырые" шаблоны ключей в виде `Glob`.
    raw_key_patterns: Vec<Glob>,
    /// Скомпилированный набор шаблонов ключей.
    pub key_patterns: GlobSet,
    raw_deny_key_patterns: Vec<Glob>,
    pub deny_key_patterns: GlobSet,
    /// "Сырые" шаблоны каналов в виде `Glob`.
    raw_channel_patterns: Vec<Glob>,
    /// Скомпилированный набор шаблонов каналов.
    pub channel_patterns: GlobSet,
    raw_deny_channel_patterns: Vec<Glob>,
    pub deny_channel_patterns: GlobSet,
}

/// Основная структура для управления ACL (Access Control List).
#[derive(Default, Debug)]
pub struct Acl {
    users: RwLock<HashMap<String, Arc<RwLock<AclUser>>>>,
}

impl AclUser {
    /// Создает нового пользователя ACL с заданным именем.
    pub fn new(username: &str) -> Result<Self, AclError> {
        let key_patterns = GlobSetBuilder::new()
            .build()
            .map_err(|_| AclError::InvalidAclRule("init key glob".into()))?;
        let deny_key_patterns = GlobSetBuilder::new()
            .build()
            .map_err(|_| AclError::InvalidAclRule("init deny key glob".into()))?;
        let channel_patterns = GlobSetBuilder::new()
            .build()
            .map_err(|_| AclError::InvalidAclRule("init chan glob".into()))?;
        let deny_channel_patterns = GlobSetBuilder::new()
            .build()
            .map_err(|_| AclError::InvalidAclRule("init deny chan glob".into()))?;

        let mut u = AclUser {
            username: username.to_string(),
            enabled: true,
            password_hashes: Vec::new(),
            allowed_categories: CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN,
            allowed_commands: HashSet::new(),
            denied_commands: HashSet::new(),

            raw_key_patterns: vec![Glob::new("*").unwrap()],
            key_patterns,

            raw_deny_key_patterns: Vec::new(),
            deny_key_patterns,

            raw_channel_patterns: vec![Glob::new("*").unwrap()],
            channel_patterns,

            raw_deny_channel_patterns: Vec::new(),
            deny_channel_patterns,
        };

        // по умолчанию разрешаем всё (эквивалент +@all, ~*)
        u.allowed_categories = CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN;
        // Разрешаем все ключи по умолчанию.
        u.raw_key_patterns.push(Glob::new("*").unwrap());
        u.rebuild_key_patterns()?;
        // Разрешаем все каналы по умолчанию.
        u.raw_channel_patterns.push(Glob::new("*").unwrap());
        u.rebuild_channel_patterns()?;
        Ok(u)
    }

    /// Перестраивает компиляцию шаблонов для ключей.
    fn rebuild_key_patterns(&mut self) -> Result<(), AclError> {
        let mut b = GlobSetBuilder::new();
        for g in &self.raw_key_patterns {
            b.add(g.clone());
        }
        self.key_patterns = b
            .build()
            .map_err(|_| AclError::InvalidAclRule("bad key glob".into()))?;
        Ok(())
    }

    fn rebuild_deny_key_patterns(&mut self) -> Result<(), AclError> {
        let mut b = GlobSetBuilder::new();
        for g in &self.raw_deny_key_patterns {
            b.add(g.clone());
        }
        self.deny_key_patterns = b
            .build()
            .map_err(|_| AclError::InvalidAclRule("bad deny-key glob".into()))?;
        Ok(())
    }

    /// Перестраивает компиляцию шаблонов для каналов.
    fn rebuild_channel_patterns(&mut self) -> Result<(), AclError> {
        let mut b = GlobSetBuilder::new();
        for g in &self.raw_channel_patterns {
            b.add(g.clone());
        }
        self.channel_patterns = b
            .build()
            .map_err(|_| AclError::InvalidAclRule("bad channel glob".into()))?;
        Ok(())
    }

    fn rebuild_deny_channel_patterns(&mut self) -> Result<(), AclError> {
        let mut b = GlobSetBuilder::new();
        for g in &self.raw_deny_channel_patterns {
            b.add(g.clone());
        }
        self.deny_channel_patterns = b
            .build()
            .map_err(|_| AclError::InvalidAclRule("bad deny-channel glob".into()))?;
        Ok(())
    }

    /// Проверяет, имеет ли пользователь право выполнить команду.
    pub fn check_permission(&self, category: &str, command: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let cmd = command.to_lowercase();
        if self.denied_commands.contains(&cmd) {
            return false;
        }
        let cat = match category {
            "read" => CmdCategory::READ,
            "write" => CmdCategory::WRITE,
            "admin" => CmdCategory::ADMIN,
            _ => CmdCategory::empty(),
        };
        if self.allowed_categories.contains(cat) {
            return true;
        }
        self.allowed_commands.contains(&cmd)
    }

    /// Проверяет, разрешён ли доступ к заданному ключу.
    pub fn check_key(&self, key: &str) -> bool {
        if !self.enabled {
            return false;
        }
        // Сначала отрицательные шаблоны
        if self.deny_key_patterns.is_match(key) {
            return false;
        }
        // Потом положительные
        self.key_patterns.is_match(key)
    }

    /// Проверяет доступность Pub/Sub-канала на основе заданных шаблонов.
    pub fn check_channel(&self, channel: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if self.deny_channel_patterns.is_match(channel) {
            return false;
        }
        self.channel_patterns.is_match(channel)
    }
}

impl Acl {
    /// Устанавливает или обновляет пользователя с набором правил ACL.
    pub fn acl_setuser(&self, username: &str, rules: &[&str]) -> Result<(), AclError> {
        // Сначала парсим все строки-правила в enum-значения
        let parsed: Vec<AclRule> = rules.iter().map(|s| s.parse()).collect::<Result<_, _>>()?;

        let mut users = self.users.write().unwrap();
        let user_arc = users
            .entry(username.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(AclUser::new(username).unwrap())))
            .clone();

        let mut user = user_arc.write().unwrap();

        // Очищаем прежние настройки (за исключением имени пользователя).
        user.enabled = false;
        user.password_hashes.clear();
        user.allowed_categories = CmdCategory::empty();
        user.allowed_commands.clear();
        user.denied_commands.clear();
        user.raw_key_patterns.clear();
        user.raw_channel_patterns.clear();
        // Сбрасываем скомпилированные наборы шаблонов, ловя ошибки
        user.key_patterns = GlobSetBuilder::new()
            .build()
            .map_err(|_| AclError::InvalidAclRule("reset key glob".into()))?;
        user.channel_patterns = GlobSetBuilder::new()
            .build()
            .map_err(|_| AclError::InvalidAclRule("reset channel glob".into()))?;

        for rule in parsed {
            match rule {
                AclRule::On => user.enabled = true,
                AclRule::Off => user.enabled = false,
                AclRule::PasswordHash(h) => user.password_hashes.push(h),
                AclRule::AllowCategory(c) => user.allowed_categories |= c,
                AclRule::DenyCategory(c) => user.allowed_categories.remove(c),
                AclRule::AllowCommand(c) => {
                    user.allowed_commands.insert(c);
                }
                AclRule::DenyCommand(c) => {
                    user.denied_commands.insert(c);
                }
                AclRule::AllowKeyPattern(pat) => {
                    user.raw_key_patterns
                        .push(Glob::new(&pat).map_err(|_| AclError::InvalidAclRule(pat.clone()))?);
                    user.rebuild_key_patterns()?;
                }
                AclRule::DenyKeyPattern(pat) => {
                    user.raw_deny_key_patterns
                        .push(Glob::new(&pat).map_err(|_| AclError::InvalidAclRule(pat.clone()))?);
                    user.rebuild_deny_key_patterns()?;
                }
                AclRule::AllowChannelPattern(pat) => {
                    user.raw_channel_patterns
                        .push(Glob::new(&pat).map_err(|_| AclError::InvalidAclRule(pat.clone()))?);
                    user.rebuild_channel_patterns()?;
                }
                AclRule::DenyChannelPattern(pat) => {
                    user.raw_deny_channel_patterns
                        .push(Glob::new(&pat).map_err(|_| AclError::InvalidAclRule(pat.clone()))?);
                    user.rebuild_deny_channel_patterns()?;
                }
            }
        }

        Ok(())
    }

    /// Возвращает копию данных пользователя ACL по его имени.
    pub fn acl_getuser(&self, username: &str) -> Option<AclUser> {
        self.users
            .read()
            .unwrap()
            .get(username)
            .map(|u| u.read().unwrap().clone())
    }

    /// Удаляет пользователя ACL по его имени.
    pub fn acl_deluser(&self, username: &str) -> Result<(), AclError> {
        let removed = self.users.write().unwrap().remove(username);
        if removed.is_some() {
            Ok(())
        } else {
            Err(AclError::UserNotFound)
        }
    }

    /// Возвращает список имен всех зарегистрированных пользователей ACL.
    pub fn acl_users(&self) -> Vec<String> {
        self.users.read().unwrap().keys().cloned().collect()
    }
}

impl FromStr for AclRule {
    type Err = AclError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "on" {
            return Ok(AclRule::On);
        }
        if s == "off" {
            return Ok(AclRule::Off);
        }
        let first = s.chars().next().unwrap();
        let rest = &s[1..];
        match first {
            '>' => Ok(AclRule::PasswordHash(rest.to_string())),
            '+' => {
                if rest.starts_with('@') {
                    let cat = match &rest[1..] {
                        "read" => CmdCategory::READ,
                        "write" => CmdCategory::WRITE,
                        "admin" => CmdCategory::ADMIN,
                        "all" => CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN,
                        other => return Err(AclError::InvalidAclRule(other.into())),
                    };
                    Ok(AclRule::AllowCategory(cat))
                } else {
                    Ok(AclRule::AllowCommand(rest.to_lowercase()))
                }
            }
            '-' => {
                if rest.starts_with('@') {
                    let cat = match &rest[1..] {
                        "read" => CmdCategory::READ,
                        "write" => CmdCategory::WRITE,
                        "admin" => CmdCategory::ADMIN,
                        other => return Err(AclError::InvalidAclRule(other.into())),
                    };
                    Ok(AclRule::DenyCategory(cat))
                } else if rest.starts_with('~') {
                    Ok(AclRule::DenyKeyPattern(rest[1..].to_string())) // ←—
                } else if rest.starts_with('&') {
                    Ok(AclRule::DenyChannelPattern(rest[1..].to_string())) // ←—
                } else {
                    Ok(AclRule::DenyCommand(rest.to_lowercase()))
                }
            }
            '~' => Ok(AclRule::AllowKeyPattern(rest.to_string())),
            '&' => Ok(AclRule::AllowChannelPattern(rest.to_string())),
            _ => Err(AclError::InvalidAclRule(s.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверяет, что пользователь по умолчанию (`AclUser::new`) включён,
    /// имеет доступ ко всем категориям, командам, ключам и каналам.
    #[test]
    fn default_user_allows_everything() {
        // AclUser::new создаёт пользователя с enabled = true, разрешив все категории
        // и установив шаблоны "*", что позволяет доступ ко всему.
        let user = AclUser::new("u").unwrap();

        // категории.
        assert!(user.check_permission("read", "wharever"));
        assert!(user.check_permission("write", "any"));
        assert!(user.check_permission("admin", "config"));

        // команды.
        assert!(user.check_permission("any", "randomcmd"));

        // ключи.
        assert!(user.check_key("kin"));
        assert!(user.check_key("za:za"));

        // каналы.
        assert!(user.check_channel("chan1"));
        assert!(user.check_channel("anything"));
    }

    /// Проверяет применение правил:
    /// включение пользователя, разрешение категории @read, добавление команды +get,
    /// запрет команды -del. Проверяем приоритеты между категориями и командами.
    #[test]
    fn setuser_read_and_individual_commands() {
        let acl = Acl::default();
        let rules = vec![
            "on",     // включаем пользователя
            "+@read", // разрешаем все команды категории read
            "+get",   // разрешаем команду get в любом случае
            "-del",   // запрещаем команду del
        ];
        acl.acl_setuser("anton", &rules).unwrap();
        let u = acl.acl_getuser("anton").unwrap();

        // read-команды любые
        assert!(u.check_permission("read", "kin"));
        // write-команды не из списка
        assert!(!u.check_permission("write", "kin"));
        // но конкретно get разрешён
        assert!(u.check_permission("write", "get"));
        // а del — запрещён
        assert!(!u.check_permission("read", "del"));
        assert!(!u.check_permission("write", "del"));
    }

    /// Проверяет работу шаблонов ключей (~pattern) и каналов (&pattern).
    #[test]
    fn key_and_channel_patterns() {
        let acl = Acl::default();
        let rules = vec![
            "on",
            "~data:*", // разрешаем ключи, начинающиеся с "data:"
            "&chan?",  // разрешаем каналы, соответствующие шаблону "chan?" (например, chan1, chanA)
        ];
        acl.acl_setuser("anton", &rules).unwrap();
        let u = acl.acl_getuser("anton").unwrap();

        // ключи
        assert!(u.check_key("data:123"));
        assert!(u.check_key("data:"));
        assert!(!u.check_key("other:100"));
        // каналы
        assert!(u.check_channel("chan1"));
        assert!(u.check_channel("chanX"));
        assert!(!u.check_channel("channel"));
    }

    /// Проверяет, что выключенный пользователь (`off`) не имеет доступ ни к чему,
    /// даже если разрешены все команды и шаблоны.
    #[test]
    fn disabling_user_blocks_everything() {
        let acl = Acl::default();
        let rules = vec![
            "off",   // выключаем пользователя
            "+@all", // правило, которое должно сработать, если бы пользователь был включён
            "~*",    // универсальный шаблон для ключей
        ];
        acl.acl_setuser("anton", &rules).unwrap();
        let u = acl.acl_getuser("anton").unwrap();

        assert!(!u.check_permission("read", "get"));
        assert!(!u.check_key("any"));
        assert!(!u.check_channel("chan"));
    }

    /// Проверяет, что пользователь удаляется и больше не доступен через `acl_getuser`.
    #[test]
    fn removing_user_works() {
        let acl = Acl::default();
        acl.acl_setuser("anton", &["on", "+@read"]).unwrap();
        assert!(acl.acl_getuser("anton").is_some());
        acl.acl_deluser("anton").unwrap();
        assert!(acl.acl_getuser("anton").is_none());
    }

    /// Проверяет, что при установке неизвестного правила возвращается ошибка `InvalidAclRule`.
    #[test]
    fn unknown_rule_returns_error() {
        let acl = Acl::default();
        let err = acl.acl_setuser("anton", &["on", "kin=zaza"]);
        assert!(matches!(err, Err(AclError::InvalidAclRule(_))));
    }

    #[test]
    fn test_create_user_and_check_defaults() {
        let user = AclUser::new("anton").unwrap();
        assert_eq!(user.username, "anton");
        assert!(user.enabled);
        assert!(user.check_permission("read", "get"));
        assert!(user.check_permission("write", "set"));
        assert!(user.check_permission("admin", "acl"));
        assert!(user.check_key("anykey"));
        assert!(user.check_channel("anychannel"));
    }

    #[test]
    fn test_acl_overwrite_existing_user() {
        let acl = Acl::default();
        acl.acl_setuser("anton", &["on", "+@read", "+get", "~x*"])
            .unwrap();

        let user1 = acl.acl_getuser("anton").unwrap();
        assert!(user1.check_permission("read", "get"));
        assert!(user1.check_key("x42"));
        assert!(!user1.check_key("y42"));

        // Перезаписываем правила
        acl.acl_setuser("anton", &["on", "+@write", "-get", "~y*"])
            .unwrap();

        let user2 = acl.acl_getuser("anton").unwrap();
        assert!(!user2.check_permission("read", "get")); // теперь запрещено
        assert!(user2.check_permission("write", "set"));
        assert!(user2.check_key("y99"));
        assert!(!user2.check_key("x99")); // старый паттерн больше не действует
    }

    #[test]
    fn test_acl_user_deletion_and_listing() {
        let acl = Acl::default();
        acl.acl_setuser("anton", &["on", "+@read"]).unwrap();
        acl.acl_setuser("boris", &["on", "+@write"]).unwrap();

        let users = acl.acl_users();
        assert!(users.contains(&"anton".to_string()));
        assert!(users.contains(&"boris".to_string()));

        acl.acl_deluser("anton").unwrap();
        let users = acl.acl_users();
        assert!(!users.contains(&"anton".to_string()));
        assert!(users.contains(&"boris".to_string()));
    }

    #[test]
    fn test_multiple_users() {
        let acl = Acl::default();

        // Устанавливаем разные правила для двух пользователей
        acl.acl_setuser("user1", &["on", "+@read", "~data:*"])
            .unwrap();
        acl.acl_setuser("user2", &["on", "+@write", "&chan?"])
            .unwrap();

        // Проверяем, что user1 имеет доступ к ключам, начинающимся с "data:"
        let user1 = acl.acl_getuser("user1").unwrap();
        assert!(user1.check_key("data:123"));
        assert!(!user1.check_key("other:100"));

        // Проверяем, что user2 имеет доступ к каналам, начинающимся с "chan"
        let user2 = acl.acl_getuser("user2").unwrap();
        assert!(user2.check_channel("chan1"));
        assert!(!user2.check_channel("channel"));
    }

    #[test]
    fn test_multiple_rules() {
        let acl = Acl::default();
        let rules = vec![
            "on",      // включаем пользователя
            "+@read",  // разрешаем все команды категории read
            "+get",    // разрешаем команду get в любом случае
            "-set",    // запрещаем команду set
            "~data:*", // разрешаем ключи, начинающиеся с "data:"
            "&chan?",  // разрешаем каналы, начинающиеся с "chan"
        ];
        acl.acl_setuser("user", &rules).unwrap();

        let u = acl.acl_getuser("user").unwrap();

        // Проверяем разрешения
        assert!(u.check_permission("read", "get"));
        assert!(!u.check_permission("write", "set"));
        assert!(u.check_key("data:123"));
        assert!(!u.check_key("other:100"));
        assert!(u.check_channel("chan1"));
        assert!(!u.check_channel("channel"));
    }

    #[test]
    fn test_user_with_multiple_passwords() {
        let acl = Acl::default();
        let rules = vec![
            "on", ">hash1", // хэш пароля 1
            ">hash2", // хэш пароля 2
        ];
        acl.acl_setuser("user", &rules).unwrap();
        let u = acl.acl_getuser("user").unwrap();

        // Проверяем, что пароли добавлены
        assert_eq!(u.password_hashes.len(), 2);
        assert!(u.password_hashes.contains(&"hash1".to_string()));
        assert!(u.password_hashes.contains(&"hash2".to_string()));
    }

    #[test]
    fn test_acl_deluser_removes_user() {
        let acl = Acl::default();
        acl.acl_setuser("user", &["on", "+@read"]).unwrap();

        // Удаляем пользователя
        acl.acl_deluser("user").unwrap();

        // Проверяем, что пользователь больше не существует
        assert!(acl.acl_getuser("user").is_none());
    }
}
