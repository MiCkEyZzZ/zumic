use globset::{Glob, GlobSet, GlobSetBuilder};

use std::collections::HashMap;
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
    }
}

/// Список всех команд и их индексы для битовой маски.
static COMMAND_INDEX: phf::Map<&'static str, usize> = phf::phf_map! {
    "get" => 0,
    "set" => 1,
    "del" => 2,
    "flushall" => 3,
    // тут можно добавить в будущем остальные команды с уникаьным индексом.
};

/// Представляет одно ACL-правило, разобранное из строки конфигурации.
#[derive(Debug)]
#[repr(u8)]
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
    AllowCommand(usize),
    /// Запретить конкретную команду (`-flushall`, `-mset`).
    DenyCommand(usize),
    /// Добавить шаблон ключей (`~pattern`).
    AllowKeyPattern(String),
    /// Запретить шаблон ключей (`-~pattern`).
    DenyKeyPattern(String),
    /// Добавить шаблон каналов Pub/Sub (`&pattern`).
    AllowChannelPattern(String),
    /// Запретить шаблон каналов Pub/Sub (`-&pattern`).
    DenyChannelPattern(String),
    /// Пользовател. не требуется пароля (nopass).
    NoPass,
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
    pub allowed_commands: u128,
    /// Запрещённые конкретные команды (например, `-flushall`).
    pub denied_commands: u128,
    /// "Сырые" шаблоны ключей в виде `Glob`.
    raw_key_patterns: Vec<Glob>,
    /// Скомпилированный набор шаблонов ключей.
    pub key_patterns: GlobSet,
    /// "Сырые" шаблоны запрещённых ключей в виде `Glob`.
    raw_deny_key_patterns: Vec<Glob>,
    /// Скомпилированный набор запрещённых ключей.
    pub deny_key_patterns: GlobSet,
    /// "Сырые" шаблоны каналов в виде `Glob`.
    raw_channel_patterns: Vec<Glob>,
    /// Скомпилированный набор шаблонов каналов.
    pub channel_patterns: GlobSet,
    /// "Сырые" шаблоны запрещённых каналов в виде `Glob`.
    raw_deny_channel_patterns: Vec<Glob>,
    /// Скомпилированный набор запрещённых каналов.
    pub deny_channel_patterns: GlobSet,

    dirty_key: bool,
    dirty_deny_key: bool,
    dirty_channel: bool,
    dirty_deny_channel: bool,
}

/// Основная структура для управления ACL (Access Control List).
#[derive(Default, Debug)]
pub struct Acl {
    users: RwLock<HashMap<String, Arc<RwLock<AclUser>>>>,
}

impl AclUser {
    /// Создает нового пользователя ACL с заданным именем.
    pub fn new(username: &str) -> Result<Self, AclError> {
        let mut u = AclUser {
            username: username.to_string(),
            enabled: true,
            password_hashes: Vec::new(),
            allowed_categories: CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN,

            allowed_commands: 0,
            denied_commands: 0,

            raw_key_patterns: vec![Glob::new("*").unwrap()],
            key_patterns: GlobSetBuilder::new().build().unwrap(),

            raw_deny_key_patterns: vec![],
            deny_key_patterns: GlobSetBuilder::new().build().unwrap(),

            raw_channel_patterns: vec![Glob::new("*").unwrap()],
            channel_patterns: GlobSetBuilder::new().build().unwrap(),

            raw_deny_channel_patterns: vec![],
            deny_channel_patterns: GlobSetBuilder::new().build().unwrap(),

            dirty_key: false,
            dirty_deny_key: false,
            dirty_channel: false,
            dirty_deny_channel: false,
        };

        u.rebuild_key_patterns()?;
        u.rebuild_deny_key_patterns()?;
        u.rebuild_channel_patterns()?;
        u.rebuild_deny_channel_patterns()?;

        Ok(u)
    }

    /// Добавляет новый шаблон ключей для разрешения и помечает паттерны "dirty".
    pub fn allow_key_pattern(&mut self, part: &str) -> Result<(), AclError> {
        let g = Glob::new(part).map_err(|_| AclError::InvalidAclRule(part.into()))?;
        self.raw_key_patterns.push(g);
        self.dirty_key = true;
        Ok(())
    }

    /// Добавляет новый шаблон ключей в список запретов и помечает паттерны "dirty".
    pub fn deny_key_pattern(&mut self, pat: &str) -> Result<(), AclError> {
        let g = Glob::new(pat).map_err(|_| AclError::InvalidAclRule(pat.into()))?;
        self.raw_deny_key_patterns.push(g);
        self.dirty_deny_key = true;
        Ok(())
    }

    /// Добавляет новый шаблон каналов для разрешения и помечает паттерны "dirty".
    pub fn allow_channel_pattern(&mut self, pat: &str) -> Result<(), AclError> {
        let g = Glob::new(pat).map_err(|_| AclError::InvalidAclRule(pat.into()))?;
        self.raw_channel_patterns.push(g);
        self.dirty_channel = true;
        Ok(())
    }

    /// Добавляет новый шаблон каналов в список запретов и помечает паттерны "dirty".
    pub fn deny_channel_pattern(&mut self, pat: &str) -> Result<(), AclError> {
        let g = Glob::new(pat).map_err(|_| AclError::InvalidAclRule(pat.into()))?;
        self.raw_deny_channel_patterns.push(g);
        self.dirty_deny_channel = true;
        Ok(())
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

    /// Перестраивает компиляцию шаблонов запрещённых ключей.
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

    /// Перестраивает компиляцию шаблонов запрещённых каналов.
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
        // Если пользователь отключён — сразу отказ.
        if !self.enabled {
            return false;
        }

        // Определяем категорию.
        let cat = match category {
            "read" => CmdCategory::READ,
            "write" => CmdCategory::WRITE,
            "admin" => CmdCategory::ADMIN,
            _ => CmdCategory::empty(),
        };

        // Приводим команду к нижнему регистру (для lookup в COMMAND_INDEX).
        let cmd_lower = command.to_lowercase();

        // Если команда известна (есть в COMMAND_INDEX) — работаем через битовые маски.
        if let Some(&idx) = COMMAND_INDEX.get(cmd_lower.as_str()) {
            let bit = 1u128 << idx;

            // 1) Запрет имеет приоритет.
            if self.denied_commands & bit != 0 {
                return false;
            }
            // 2) Глобальная категория.
            if self.allowed_categories.contains(cat) {
                return true;
            }
            // 3) Специальное разрешение по биту.
            return self.allowed_commands & bit != 0;
        }

        // Для неизвестных команд — только по категории.
        self.allowed_categories.contains(cat)
    }

    /// Проверяет, разрешён ли доступ к заданному ключу.
    pub fn check_key(&mut self, key: &str) -> bool {
        if self.dirty_key {
            self.rebuild_key_patterns().unwrap();
            self.dirty_key = false;
        }
        if self.dirty_deny_key {
            self.rebuild_deny_key_patterns().unwrap();
            self.dirty_deny_key = false;
        }
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
    pub fn check_channel(&mut self, channel: &str) -> bool {
        if self.dirty_deny_channel {
            self.rebuild_deny_channel_patterns().unwrap();
            self.dirty_deny_channel = false;
        }
        if self.dirty_channel {
            self.rebuild_channel_patterns().unwrap();
            self.dirty_channel = false;
        }

        if !self.enabled {
            return false;
        }
        if self.deny_channel_patterns.is_match(channel) {
            return false;
        }
        self.channel_patterns.is_match(channel)
    }

    /// Сбрасывает все правила ACL, оставляя только username.
    pub fn reset_rules(&mut self) {
        // Сброс флагов доступа и списков
        self.enabled = false;
        self.password_hashes.clear();
        self.allowed_categories = CmdCategory::empty();
        self.allowed_commands = 0;
        self.denied_commands = 0;

        // Очищаем "сырые" паттерны
        self.raw_key_patterns.clear();
        self.raw_deny_key_patterns.clear();
        self.raw_channel_patterns.clear();
        self.raw_deny_channel_patterns.clear();

        // Отмечаем, что все четыре набора паттернов нужно пересобрать при следующих проверках
        self.dirty_key = true;
        self.dirty_deny_key = true;
        self.dirty_channel = true;
        self.dirty_deny_channel = true;
    }
}

impl Acl {
    /// Устанавливает или обновляет пользователя с набором правил ACL.
    pub fn acl_setuser(&self, username: &str, rules: &[&str]) -> Result<(), AclError> {
        // Сначала парсим все строки-правила в enum-значения
        let parsed: Vec<AclRule> = rules.iter().map(|s| s.parse()).collect::<Result<_, _>>()?;

        // Получаем либо создаём пользователя
        let mut users = self.users.write().unwrap();
        let user_arc = users
            .entry(username.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(AclUser::new(username).unwrap())))
            .clone();

        let mut user = user_arc.write().unwrap();

        // Очищаем прежние настройки (за исключением имени пользователя).
        user.reset_rules();

        for rule in parsed {
            match rule {
                AclRule::On => user.enabled = true,
                AclRule::Off => user.enabled = false,
                AclRule::PasswordHash(h) => user.password_hashes.push(h),
                AclRule::AllowCategory(c) => user.allowed_categories |= c,
                AclRule::DenyCategory(c) => user.allowed_categories.remove(c),
                AclRule::AllowCommand(idx) => {
                    user.allowed_commands |= 1u128 << idx;
                }
                AclRule::DenyCommand(idx) => {
                    user.denied_commands |= 1u128 << idx;
                }
                AclRule::AllowKeyPattern(p) => user.allow_key_pattern(&p)?,
                AclRule::DenyKeyPattern(p) => user.deny_key_pattern(&p)?,
                AclRule::AllowChannelPattern(p) => user.allow_channel_pattern(&p)?,
                AclRule::DenyChannelPattern(p) => user.deny_channel_pattern(&p)?,
                AclRule::NoPass => {
                    // nopass: не добавляем в password_hashes — authenticate увидит пустой список и пропустит логин
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
        if self.users.write().unwrap().remove(username).is_some() {
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
        if s == "nopass" {
            return Ok(AclRule::NoPass);
        }

        let first = s
            .chars()
            .next()
            .ok_or_else(|| AclError::InvalidAclRule(s.into()))?;
        let rest = &s[1..];

        match first {
            '>' => Ok(AclRule::PasswordHash(rest.to_string())),
            '+' if rest.starts_with('@') => {
                let cat = match &rest[1..] {
                    "read" => CmdCategory::READ,
                    "write" => CmdCategory::WRITE,
                    "admin" => CmdCategory::ADMIN,
                    "all" => CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN,
                    o => return Err(AclError::InvalidAclRule(o.into())),
                };
                Ok(AclRule::AllowCategory(cat))
            }
            '+' => {
                let cmd = rest.to_lowercase();
                let &idx = COMMAND_INDEX
                    .get(cmd.as_str())
                    .ok_or_else(|| AclError::InvalidAclRule(cmd.clone()))?;
                Ok(AclRule::AllowCommand(idx))
            }
            '-' if rest.starts_with('@') => {
                let cat = match &rest[1..] {
                    "read" => CmdCategory::READ,
                    "write" => CmdCategory::WRITE,
                    "admin" => CmdCategory::ADMIN,
                    o => return Err(AclError::InvalidAclRule(o.into())),
                };
                Ok(AclRule::DenyCategory(cat))
            }
            '-' => {
                if rest.starts_with('~') {
                    return Ok(AclRule::DenyKeyPattern(rest[1..].to_string()));
                }
                if rest.starts_with('&') {
                    return Ok(AclRule::DenyChannelPattern(rest[1..].to_string()));
                }
                let cmd = rest.to_lowercase();
                let &idx = COMMAND_INDEX
                    .get(cmd.as_str())
                    .ok_or_else(|| AclError::InvalidAclRule(cmd.clone()))?;
                Ok(AclRule::DenyCommand(idx))
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

    /// Тест проверяет, что пользователь по умолчанию (`AclUser::new`) включён,
    /// имеет доступ ко всем категориям, командам, ключам и каналам.
    #[test]
    fn default_user_allows_everything() {
        // AclUser::new создаёт пользователя с enabled = true, разрешив все категории
        // и установив шаблоны "*", что позволяет доступ ко всему.
        let mut user = AclUser::new("u").unwrap();

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

    /// Тест проверяет применение правил:
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

    /// Тест проверяет работу шаблонов ключей (~pattern) и каналов (&pattern).
    #[test]
    fn key_and_channel_patterns() {
        let acl = Acl::default();
        let rules = vec![
            "on",
            "~data:*", // разрешаем ключи, начинающиеся с "data:"
            "&chan?",  // разрешаем каналы, соответствующие шаблону "chan?" (например, chan1, chanA)
        ];
        acl.acl_setuser("anton", &rules).unwrap();
        let mut u = acl.acl_getuser("anton").unwrap();

        // ключи
        assert!(u.check_key("data:123"));
        assert!(u.check_key("data:"));
        assert!(!u.check_key("other:100"));
        // каналы
        assert!(u.check_channel("chan1"));
        assert!(u.check_channel("chanX"));
        assert!(!u.check_channel("channel"));
    }

    /// Тест проверяет, что выключенный пользователь (`off`) не имеет доступ ни к чему,
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
        let mut u = acl.acl_getuser("anton").unwrap();

        assert!(!u.check_permission("read", "get"));
        assert!(!u.check_key("any"));
        assert!(!u.check_channel("chan"));
    }

    /// Тест проверяет, что пользователь удаляется и больше не доступен через `acl_getuser`.
    #[test]
    fn removing_user_works() {
        let acl = Acl::default();
        acl.acl_setuser("anton", &["on", "+@read"]).unwrap();
        assert!(acl.acl_getuser("anton").is_some());
        acl.acl_deluser("anton").unwrap();
        assert!(acl.acl_getuser("anton").is_none());
    }

    /// Тест проверяет, что при установке неизвестного правила возвращается ошибка `InvalidAclRule`.
    #[test]
    fn unknown_rule_returns_error() {
        let acl = Acl::default();
        let err = acl.acl_setuser("anton", &["on", "kin=dzadza"]);
        assert!(matches!(err, Err(AclError::InvalidAclRule(_))));
    }

    /// Тест проверяет, что `AclUser::new` создаёт пользователя с корректными значениями по умолчанию.
    #[test]
    fn test_create_user_and_check_defaults() {
        let mut user = AclUser::new("anton").unwrap();
        assert_eq!(user.username, "anton");
        assert!(user.enabled);
        assert!(user.check_permission("read", "get"));
        assert!(user.check_permission("write", "set"));
        assert!(user.check_permission("admin", "acl"));
        assert!(user.check_key("anykey"));
        assert!(user.check_channel("anychannel"));
    }

    /// Тест проверяет перезапись правил пользователя при повторном вызове `acl_setuser`.
    #[test]
    fn test_acl_overwrite_existing_user() {
        let acl = Acl::default();
        acl.acl_setuser("anton", &["on", "+@read", "+get", "~x*"])
            .unwrap();

        let mut user1 = acl.acl_getuser("anton").unwrap();
        assert!(user1.check_permission("read", "get"));
        assert!(user1.check_key("x42"));
        assert!(!user1.check_key("y42"));

        // Перезаписываем правила
        acl.acl_setuser("anton", &["on", "+@write", "-get", "~y*"])
            .unwrap();

        let mut user2 = acl.acl_getuser("anton").unwrap();
        assert!(!user2.check_permission("read", "get")); // теперь запрещено
        assert!(user2.check_permission("write", "set"));
        assert!(user2.check_key("y99"));
        assert!(!user2.check_key("x99")); // старый паттерн больше не действует
    }

    /// Тест проверяет список пользователей и удаление через `acl_deluser`.
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

    /// Тест проверяет независимость ACL-настроек для нескольких пользователей.
    #[test]
    fn test_multiple_users() {
        let acl = Acl::default();

        // Устанавливаем разные правила для двух пользователей
        acl.acl_setuser("user1", &["on", "+@read", "~data:*"])
            .unwrap();
        acl.acl_setuser("user2", &["on", "+@write", "&chan?"])
            .unwrap();

        // Проверяем, что user1 имеет доступ к ключам, начинающимся с "data:"
        let mut user1 = acl.acl_getuser("user1").unwrap();
        assert!(user1.check_key("data:123"));
        assert!(!user1.check_key("other:100"));

        // Проверяем, что user2 имеет доступ к каналам, начинающимся с "chan"
        let mut user2 = acl.acl_getuser("user2").unwrap();
        assert!(user2.check_channel("chan1"));
        assert!(!user2.check_channel("channel"));
    }

    /// Тест проверяет применение нескольких типов правил одновременно.
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

        let mut u = acl.acl_getuser("user").unwrap();

        // Проверяем разрешения
        assert!(u.check_permission("read", "get"));
        assert!(!u.check_permission("write", "set"));
        assert!(u.check_key("data:123"));
        assert!(!u.check_key("other:100"));
        assert!(u.check_channel("chan1"));
        assert!(!u.check_channel("channel"));
    }

    /// Тест проверяет добавление нескольких хэшей паролей через правила ">hash".
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

    /// Тест проверяет удаление пользователя через `acl_deluser`.
    #[test]
    fn test_acl_deluser_removes_user() {
        let acl = Acl::default();
        acl.acl_setuser("user", &["on", "+@read"]).unwrap();

        // Удаляем пользователя
        acl.acl_deluser("user").unwrap();

        // Проверяем, что пользователь больше не существует
        assert!(acl.acl_getuser("user").is_none());
    }

    /// Тест проверяет, что ленивый rebuild паттерн ключей отрабатывает один раз.
    #[test]
    fn lazy_key_rebuild_once() {
        let mut user = AclUser::new("u").unwrap();
        // изначально key_patterns = "*"
        // добавим два новых шаблона.
        user.allow_key_pattern("kin").unwrap();
        user.allow_key_pattern("dzadza").unwrap();
        // первый вызов check_key должен пересобрать паттерн
        assert!(user.check_key("kin123"));
        // dirty_key сбросился, второй вызов тоже должен работать без паники
        assert!(user.check_key("dzadza456"));
    }

    /// Тест проверяет, что ленивый rebuild паттернов каналов отрабатывает один раз.
    #[test]
    fn lazy_channel_rebuild_once() {
        let mut user = AclUser::new("u").unwrap();
        user.allow_channel_pattern("chan").unwrap();
        user.allow_channel_pattern("topic").unwrap();
        // первый вызов пересобирает
        assert!(user.check_channel("chanXYZ"));
        // второй уже без паники.
        assert!(user.check_channel("topic123"));
    }

    /// Тест проверяет, что reset_rules очищает всё и запрещает любые операции.
    #[test]
    fn test_reset_rules_behavior() {
        let mut user = AclUser::new("u").unwrap();
        // задаём несколько правил.
        user.allow_key_pattern("kin*").unwrap();
        user.allow_channel_pattern("dzadza*").unwrap();
        user.password_hashes.push("h".into());
        // разрешаем команду "get" через битовую маску
        let idx = *COMMAND_INDEX.get("get").unwrap();
        user.allowed_commands |= 1u128 << idx;
        // сбрасываем
        user.reset_rules();
        // после reset любое разрешение должно быть запрещено.
        assert!(!user.check_permission("read", "something"));
        assert!(!user.check_key("kin123"));
        assert!(!user.check_channel("dzadzaXYZ"));
    }

    /// Тест проверяет, что добавление дубликатов паттернов не ломает сборщик.
    #[test]
    fn test_duplicate_patterns_no_panic() {
        let mut user = AclUser::new("u").unwrap();
        // дважды один и тот же шаблон.
        user.allow_key_pattern("kin*").unwrap();
        user.allow_key_pattern("kin*").unwrap();
        assert!(user.check_key("kinABC"));
        // аналогично для каналов.
        user.allow_channel_pattern("kinchan*").unwrap();
        user.allow_channel_pattern("kinchan*").unwrap();
        assert!(user.check_channel("kinchan123"));
    }

    /// Тест проверяет, что при невалидном glob метод возвращает Err, а состояние не меняется.
    #[test]
    fn test_invalid_glob_returns_err_and_state_unchanged() {
        let mut user = AclUser::new("u").unwrap();
        // невалидный паттерн.
        assert!(user.allow_key_pattern("**[invalid").is_err());
        // старый шаблон "*" должен остаться.
        assert!(user.check_key("anything"));
    }

    /// Тест проверяет, что deny перед allow для ключей.
    #[test]
    fn test_allow_and_deny_pattern_priority_key() {
        let mut user = AclUser::new("u").unwrap();
        user.allow_key_pattern("kin*").unwrap();
        user.deny_key_pattern("kindzadza*").unwrap();
        // foo123 — разрешено
        assert!(user.check_key("kin123"));
        // foobarXYZ — сначала deny, значит запрещено
        assert!(!user.check_key("kindzadzaXYZ"));
    }

    /// Тест проверяет приоритет deny перед allow для каналов.
    #[test]
    fn test_allow_and_deny_pattern_priority_channel() {
        let mut user = AclUser::new("u").unwrap();
        user.allow_channel_pattern("chan*").unwrap();
        user.deny_channel_pattern("chanbad*").unwrap();
        assert!(user.check_channel("chanGood"));
        assert!(!user.check_channel("chanbad123"));
    }
}
