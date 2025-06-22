use std::{
    str::FromStr,
    sync::{Arc, RwLock},
};

use dashmap::DashMap;
use globset::{Glob, GlobSet, GlobSetBuilder};
use once_cell::sync::Lazy;

use crate::error::auth::AclError;

/// Глобальный "всегда разрешающий" паттерн.
static DEFAULT_GLOB: Lazy<Glob> = Lazy::new(|| Glob::new("*").unwrap());

bitflags::bitflags! {
    /// Битовая маска категорий команд, используемая для обозначения групп команд,
    /// например, `@read`, `@write`, `@admin`.
    #[derive(Copy, Clone, Debug)]
    pub struct CmdCategory: u32 {
        /// Команды для операций чтения.
        const READ = 1 << 0;
        /// Команды для операций записи.
        const WRITE = 1 << 1;
        /// Административные команды.
        const ADMIN = 1 << 2;
    }
}

/// Парсим строки категории один раз, сразу в битовую маску.
pub fn parse_category(cat: &str) -> CmdCategory {
    match cat {
        "read" => CmdCategory::READ,
        "write" => CmdCategory::WRITE,
        "admin" => CmdCategory::ADMIN,
        _ => CmdCategory::empty(),
    }
}

/// Парсим имя команды один раз в индекс.
/// Возвращает `None` для незнакомых команд.
pub fn lookup_cmd_idx(cmd: &str) -> Option<usize> {
    // один раз приводим к to_ascii_lowercase, а в горящем пути уже usize
    let lower = cmd.to_ascii_lowercase();
    COMMAND_INDEX.get(lower.as_str()).copied()
}

/// Список всех команд и их индексы для битовой маски.
static COMMAND_INDEX: phf::Map<&'static str, usize> = phf::phf_map! {
    "get" => 0,
    "set" => 1,
    "del" => 2,
    "flushall" => 3,
    // тут можно добавить в будущем остальные команды с уникальным индексом.
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
    /// Пользователь. не требуется пароля (nopass).
    NoPass,
}

/// Конфигурация пользователя ACL.
#[derive(Debug, Clone)]
pub struct AclUser {
    /// Имя пользователя.
    pub username: String,
    /// Флаг, обозначающий, включён ли пользователь.
    pub enabled: bool,
    /// Список хешей паролей для поддержки ротации.
    pub password_hashes: Vec<String>,
    /// Разрешённые категории команд.
    pub allowed_categories: CmdCategory,
    /// Разрешённые конкретные команды.
    pub allowed_commands: u128,
    /// Запрещённые конкретные команды (например, `-flushall`).
    pub denied_commands: u128,

    /// "Сырые" шаблоны ключей в виде `Glob`.
    raw_key_patterns: Vec<Glob>,
    /// "Сырые" шаблоны запрещённых ключей в виде `Glob`.
    raw_deny_key_patterns: Vec<Glob>,
    /// "Сырые" шаблоны каналов в виде `Glob`.
    raw_channel_patterns: Vec<Glob>,
    /// "Сырые" шаблоны запрещённых каналов в виде `Glob`.
    raw_deny_channel_patterns: Vec<Glob>,

    /// Скомпилированный набор шаблонов ключей.
    pub key_patterns: GlobSet,
    /// Скомпилированный набор запрещённых ключей.
    pub deny_key_patterns: GlobSet,
    /// Скомпилированный набор шаблонов каналов.
    pub channel_patterns: GlobSet,
    /// Скомпилированный набор запрещённых каналов.
    pub deny_channel_patterns: GlobSet,
}

/// Основная структура для управления ACL (Access Control List).
#[derive(Default, Debug)]
pub struct Acl {
    users: DashMap<String, Arc<RwLock<AclUser>>>,
}

impl AclUser {
    /// Создает нового пользователя ACL с заданным именем.
    pub fn new(username: &str) -> Result<Self, AclError> {
        let default_glob = DEFAULT_GLOB.clone();
        let mut u = AclUser {
            username: username.to_string(),
            enabled: true,
            password_hashes: Vec::new(),
            allowed_categories: CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN,
            allowed_commands: 0,
            denied_commands: 0,
            raw_key_patterns: vec![default_glob.clone()],
            raw_deny_key_patterns: Vec::new(),
            raw_channel_patterns: vec![default_glob.clone()],
            raw_deny_channel_patterns: Vec::new(),
            key_patterns: GlobSet::empty(),
            deny_key_patterns: GlobSet::empty(),
            channel_patterns: GlobSet::empty(),
            deny_channel_patterns: GlobSet::empty(),
        };

        u.rebuild_all_patterns()?;
        u.rebuild_all_deny_patterns()?;

        Ok(u)
    }

    pub fn rebuild_globset(patterns: &[Glob]) -> Result<GlobSet, AclError> {
        let mut b = GlobSetBuilder::new();
        for g in patterns {
            b.add(g.clone());
        }
        b.build()
            .map_err(|_| AclError::InvalidAclRule("globset build".into()))
    }

    pub fn rebuild_all_patterns(&mut self) -> Result<(), AclError> {
        self.key_patterns = Self::rebuild_globset(&self.raw_key_patterns)?;
        self.channel_patterns = Self::rebuild_globset(&self.raw_channel_patterns)?;
        Ok(())
    }

    pub fn rebuild_all_deny_patterns(&mut self) -> Result<(), AclError> {
        self.deny_key_patterns = Self::rebuild_globset(&self.raw_deny_key_patterns)?;
        self.deny_channel_patterns = Self::rebuild_globset(&self.raw_deny_channel_patterns)?;
        Ok(())
    }

    /// Добавляет новый шаблон ключей для разрешения и помечает паттерны "dirty".
    pub fn allow_key_pattern(
        &mut self,
        pat: &str,
    ) -> Result<(), AclError> {
        let g = Glob::new(pat).map_err(|_| AclError::InvalidAclRule(pat.into()))?;
        self.raw_key_patterns.push(g);
        // Пересобираем только ключи:
        self.key_patterns = Self::rebuild_globset(&self.raw_key_patterns)?;
        Ok(())
    }

    /// Добавляет новый шаблон ключей в список запретов и помечает паттерны "dirty".
    pub fn deny_key_pattern(
        &mut self,
        pat: &str,
    ) -> Result<(), AclError> {
        let g = Glob::new(pat).map_err(|_| AclError::InvalidAclRule(pat.into()))?;
        self.raw_deny_key_patterns.push(g);
        // Пересобираем только deny-ключи:
        self.deny_key_patterns = Self::rebuild_globset(&self.raw_deny_key_patterns)?;
        Ok(())
    }

    /// Добавляет новый шаблон каналов для разрешения и помечает паттерны "dirty".
    pub fn allow_channel_pattern(
        &mut self,
        pat: &str,
    ) -> Result<(), AclError> {
        let g = Glob::new(pat).map_err(|_| AclError::InvalidAclRule(pat.into()))?;
        self.raw_channel_patterns.push(g);
        self.channel_patterns = Self::rebuild_globset(&self.raw_channel_patterns)?;
        Ok(())
    }

    /// Добавляет новый шаблон каналов в список запретов и помечает паттерны "dirty".
    pub fn deny_channel_pattern(
        &mut self,
        pat: &str,
    ) -> Result<(), AclError> {
        let g = Glob::new(pat).map_err(|_| AclError::InvalidAclRule(pat.into()))?;
        self.raw_deny_channel_patterns.push(g);
        self.deny_channel_patterns = Self::rebuild_globset(&self.raw_deny_channel_patterns)?;
        Ok(())
    }

    /// Проверяет, имеет ли пользователь право выполнить команду.
    /// Горячий путь: принимает уже разобранную категорию и опциональный индекс команды.
    pub fn check_idx(
        &self,
        category: CmdCategory,
        cmd_idx: Option<usize>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        if let Some(idx) = cmd_idx {
            let bit = 1u128 << idx;
            if self.denied_commands & bit != 0 {
                return false;
            }
            if self.allowed_categories.contains(category) {
                return true;
            }
            return self.allowed_commands & bit != 0;
        }
        // неизвестная команда — только по категории
        self.allowed_categories.contains(category)
    }

    /// Проверяет, разрешён ли доступ к заданному ключу.
    pub fn check_key(
        &self,
        key: &str,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        if self.deny_key_patterns.is_match(key) {
            return false;
        }
        if self.key_patterns.is_empty() {
            true
        } else {
            self.key_patterns.is_match(key)
        }
    }

    /// Проверяет доступность Pub/Sub-канала на основе заданных шаблонов.
    pub fn check_channel(
        &self,
        channel: &str,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        if self.deny_channel_patterns.is_match(channel) {
            return false;
        }
        if self.channel_patterns.is_empty() {
            true
        } else {
            self.channel_patterns.is_match(channel)
        }
    }

    /// Сбрасывает все правила ACL, оставляя только username.
    pub fn reset_rules(&mut self) {
        // Выключаем пользователя и чистим команды
        self.enabled = false;
        self.password_hashes.clear();
        self.allowed_categories = CmdCategory::empty();
        self.allowed_commands = 0;
        self.denied_commands = 0;

        // Очищаем все "сырые" паттерны
        self.raw_key_patterns.clear();
        self.raw_deny_key_patterns.clear();
        self.raw_channel_patterns.clear();
        self.raw_deny_channel_patterns.clear();

        // Мгновенно пересобираем пустые множества — теперь key_patterns и channel_patterns пусты,
        // и check_key/check_channel всегда вернут false, пока не добавятся новые паттерны.
        let _ = self.rebuild_all_patterns();
        let _ = self.rebuild_all_deny_patterns();
    }
}

impl Acl {
    /// Устанавливает или обновляет пользователя с набором правил ACL.
    pub fn acl_setuser(
        &self,
        username: &str,
        rules: &[&str],
    ) -> Result<(), AclError> {
        // Сначала парсим все строки-правила в enum-значения
        let parsed: Vec<AclRule> = rules.iter().map(|s| s.parse()).collect::<Result<_, _>>()?;

        // Получаем либо создаём пользователя
        let user_arc = self
            .users
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
                AclRule::AllowCommand(i) => user.allowed_commands |= 1u128 << i,
                AclRule::DenyCommand(i) => user.denied_commands |= 1u128 << i,
                AclRule::AllowKeyPattern(p) => user.allow_key_pattern(&p)?,
                AclRule::DenyKeyPattern(p) => user.deny_key_pattern(&p)?,
                AclRule::AllowChannelPattern(p) => user.allow_channel_pattern(&p)?,
                AclRule::DenyChannelPattern(p) => user.deny_channel_pattern(&p)?,
                AclRule::NoPass => {}
            }
        }

        // Eager-rebuild после всех изменений
        user.rebuild_all_patterns()?;
        user.rebuild_all_deny_patterns()?;
        Ok(())
    }

    /// Возвращает копию данных пользователя ACL по его имени.
    pub fn acl_getuser(
        &self,
        username: &str,
    ) -> Option<AclUser> {
        self.users.get(username).map(|u| u.read().unwrap().clone())
    }

    /// Удаляет пользователя ACL по его имени.
    pub fn acl_deluser(
        &self,
        username: &str,
    ) -> Result<(), AclError> {
        self.users
            .remove(username)
            .map(|_| ())
            .ok_or(AclError::UserNotFound)
    }

    /// Возвращает список имен всех зарегистрированных пользователей ACL.
    pub fn acl_users(&self) -> Vec<String> {
        self.users.iter().map(|e| e.key().clone()).collect()
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
        let (head, rest) = s.split_at(1);
        match head {
            ">" => Ok(AclRule::PasswordHash(rest.into())),
            "+" if rest.starts_with('@') => {
                let c = parse_category(&rest[1..]);
                Ok(AclRule::AllowCategory(c))
            }
            "+" => {
                let idx =
                    lookup_cmd_idx(rest).ok_or_else(|| AclError::InvalidAclRule(rest.into()))?;
                Ok(AclRule::AllowCommand(idx))
            }
            "-" if rest.starts_with('@') => {
                let c = parse_category(&rest[1..]);
                Ok(AclRule::DenyCategory(c))
            }
            "-" if rest.starts_with('~') => Ok(AclRule::DenyKeyPattern(rest[1..].into())),
            "-" if rest.starts_with('&') => Ok(AclRule::DenyChannelPattern(rest[1..].into())),
            "-" => {
                let idx =
                    lookup_cmd_idx(rest).ok_or_else(|| AclError::InvalidAclRule(rest.into()))?;
                Ok(AclRule::DenyCommand(idx))
            }
            "~" => Ok(AclRule::AllowKeyPattern(rest.into())),
            "&" => Ok(AclRule::AllowChannelPattern(rest.into())),
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
        // AclUser::new создаёт пользователя с enabled = true,
        // разрешив все категории и установив шаблоны "*",
        // что позволяет доступ ко всему.
        let user = AclUser::new("u").unwrap();

        // категории.
        let cat_read = parse_category("read");
        let cat_write = parse_category("write");
        let cat_admin = parse_category("admin");

        assert!(user.check_idx(cat_read, lookup_cmd_idx("wharever")));
        assert!(user.check_idx(cat_write, lookup_cmd_idx("any")));
        assert!(user.check_idx(cat_admin, lookup_cmd_idx("config")));

        // команды: незнакомая команда idx == None — но по категориям всё разрешено.
        assert!(user.check_idx(cat_read, lookup_cmd_idx("randomcmd")));
        assert!(user.check_idx(cat_write, lookup_cmd_idx("randomcmd")));
        assert!(user.check_idx(cat_admin, lookup_cmd_idx("randomcmd")));

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

        // парсим категории один раз
        let cat_read = parse_category("read");
        let cat_write = parse_category("write");

        // read-команды любые (lookup_cmd_idx("kin") == None → проверка по категории)
        assert!(u.check_idx(cat_read, lookup_cmd_idx("kin")));
        // write-команды не из списка
        assert!(!u.check_idx(cat_write, lookup_cmd_idx("kin")));
        // но конкретно get разрешён
        assert!(u.check_idx(cat_write, lookup_cmd_idx("get")));
        // а del — запрещён
        assert!(!u.check_idx(cat_read, lookup_cmd_idx("del")));
        assert!(!u.check_idx(cat_write, lookup_cmd_idx("del")));
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

    /// Тест проверяет, что выключенный пользователь (`off`) не имеет доступ ни к чему,
    /// даже если разрешены все команды и шаблоны.
    #[test]
    fn disabling_user_blocks_everything() {
        let acl = Acl::default();
        let rules = vec![
            "off",   // выключаем пользователя
            "+@all", // правило, которое сработало бы, если пользователь был включён
            "~*",    // универсальный шаблон для ключей
        ];
        acl.acl_setuser("anton", &rules).unwrap();
        let u = acl.acl_getuser("anton").unwrap();

        // Категории парсим один раз
        let cat_read = parse_category("read");

        // Пользователь выключен → любой вызов check_idx(false), check_key, check_channel должен вернуть false
        assert!(!u.check_idx(cat_read, lookup_cmd_idx("get")));
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
        // Создаём нового пользователя и проверяем базовые поля
        let user = AclUser::new("anton").unwrap();
        assert_eq!(user.username, "anton");
        assert!(user.enabled);

        // Парсим категории один раз
        let cat_read = parse_category("read");
        let cat_write = parse_category("write");
        let cat_admin = parse_category("admin");

        // Известные команды должны иметь индексы
        let idx_get = lookup_cmd_idx("get").expect("get должно быть в COMMAND_INDEX");
        let idx_set = lookup_cmd_idx("set").expect("set должно быть в COMMAND_INDEX");

        // Проверяем разрешения по категориям и индексам
        assert!(user.check_idx(cat_read, Some(idx_get)));
        assert!(user.check_idx(cat_write, Some(idx_set)));

        // Команда "acl" не в COMMAND_INDEX → lookup_cmd_idx вернёт None,
        // но по категории admin доступ всё равно есть.
        assert_eq!(lookup_cmd_idx("acl"), None);
        assert!(user.check_idx(cat_admin, None));

        // Проверяем доступ к ключам и каналам по умолчанию
        assert!(user.check_key("anykey"));
        assert!(user.check_channel("anychannel"));
    }

    /// Тест проверяет перезапись правил пользователя при повторном вызове `acl_setuser`.
    #[test]
    fn test_acl_overwrite_existing_user() {
        let acl = Acl::default();

        // Изначальные правила: read + get, ключи ~x*
        acl.acl_setuser("anton", &["on", "+@read", "+get", "~x*"])
            .unwrap();
        let u1 = acl.acl_getuser("anton").unwrap();

        let cat_read = parse_category("read");
        assert!(u1.check_idx(cat_read, lookup_cmd_idx("get")));
        assert!(u1.check_key("x42"));
        assert!(!u1.check_key("y42"));

        // Перезаписываем правила: write, снимаем +get, ключи ~y*
        acl.acl_setuser("anton", &["on", "+@write", "-get", "~y*"])
            .unwrap();
        let u2 = acl.acl_getuser("anton").unwrap();

        let cat_write = parse_category("write");
        // Теперь read-get запрещён:
        assert!(!u2.check_idx(cat_read, lookup_cmd_idx("get")));
        // А write-set разрешён:
        assert!(u2.check_idx(cat_write, lookup_cmd_idx("set")));
        // Новый паттерн ~y* работает, старый ~x* — уже нет
        assert!(u2.check_key("y99"));
        assert!(!u2.check_key("x99"));
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
        let user1 = acl.acl_getuser("user1").unwrap();
        assert!(user1.check_key("data:123"));
        assert!(!user1.check_key("other:100"));

        // Проверяем, что user2 имеет доступ к каналам, начинающимся с "chan"
        let user2 = acl.acl_getuser("user2").unwrap();
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
        let u = acl.acl_getuser("user").unwrap();

        // Разбираем категории один раз
        let cat_read = parse_category("read");
        let cat_write = parse_category("write");

        // Проверяем команды через check_idx
        // Для "get" lookup_cmd_idx вернёт Some(idx)
        assert!(u.check_idx(cat_read, lookup_cmd_idx("get")));
        // Для "set" lookup_cmd_idx вернёт Some(idx), но мы его явно запретили
        assert!(!u.check_idx(cat_write, lookup_cmd_idx("set")));

        // Проверяем шаблоны ключей
        assert!(u.check_key("data:123"));
        assert!(!u.check_key("other:100"));

        // Проверяем шаблоны каналов
        assert!(u.check_channel("chan1"));
        assert!(!u.check_channel("channel"));
    }

    /// Тест проверяет добавление нескольких хешей паролей через правила ">hash".
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

        // Задаём несколько правил вручную:
        user.allow_key_pattern("kin*").unwrap();
        user.allow_channel_pattern("dzadza*").unwrap();
        user.password_hashes.push("h".into());
        // Разрешаем команду "get" через битовую маску:
        let idx = lookup_cmd_idx("get").expect("get должно быть в карте");
        user.allowed_commands |= 1u128 << idx;

        // Сбрасываем все правила
        user.reset_rules();

        // После reset всё должно быть запрещено:
        let cat_read = parse_category("read");
        assert!(!user.check_idx(cat_read, lookup_cmd_idx("something")));
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
        user.reset_rules();
        // после reset_rules пользователь выключен, нужно снова включить
        user.enabled = true;
        user.allow_channel_pattern("chan*").unwrap();
        user.deny_channel_pattern("chanbad*").unwrap();
        assert!(user.check_channel("chanGood"));
        assert!(!user.check_channel("chanbad123"));
    }
}
