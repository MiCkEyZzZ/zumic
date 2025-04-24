use globset::{Glob, GlobSet, GlobSetBuilder};

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use super::errors::AclError;

bitflags::bitflags! {
    /// Битовая маска категорий команд (@read, @write ...).
    #[derive(Clone, Debug)]
    pub struct CmdCategory: u32 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const ADMIN = 1 << 2;
        // ... добавим категории по мере необходимости
    }
}

#[derive(Debug, Clone)]
pub struct AclUser {
    pub username: String,
    pub enabled: bool,

    /// Поддерживаем несколько старых хешей для ротации.
    pub password_hashes: Vec<String>,

    /// Разрешённые категории.
    pub allowed_categories: CmdCategory,
    /// Разрещённые конкретные команды.
    pub allowed_commands: HashSet<String>,
    /// Запрещённые конкретные команды (например, `-flushall`)
    pub denied_commands: HashSet<String>,

    /// Шаблоны ключей и каналов
    raw_key_patterns: Vec<Glob>,
    pub key_patterns: GlobSet,
    raw_channel_patterns: Vec<Glob>,
    pub channel_patterns: GlobSet,
}

#[derive(Default, Debug)]
pub struct Acl {
    users: RwLock<HashMap<String, Arc<RwLock<AclUser>>>>,
}

impl AclUser {
    pub fn new(username: &str) -> Result<Self, AclError> {
        // по умолчанию: включён, без пароля, все команды @all
        let mut user = AclUser {
            username: username.to_string(),
            enabled: true,
            password_hashes: Vec::new(),
            allowed_categories: CmdCategory::empty(),
            allowed_commands: HashSet::new(),
            denied_commands: HashSet::new(),
            raw_key_patterns: Vec::new(),
            key_patterns: GlobSetBuilder::new().build().unwrap(),
            raw_channel_patterns: Vec::new(),
            channel_patterns: GlobSetBuilder::new().build().unwrap(),
        };

        // по умолчанию разрешаем всё (эквивалент +@all, ~*)
        user.allowed_categories = CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN;
        user.raw_key_patterns.push(Glob::new("*").unwrap());
        user.rebuild_key_patterns()?;
        user.raw_channel_patterns.push(Glob::new("*").unwrap());
        user.rebuild_channel_patterns()?;
        Ok(user)
    }

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

    pub fn check_permission(&self, category: &str, command: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let cmd = command.to_lowercase();
        // 1) проверяем явный запрет
        if self.denied_commands.contains(&cmd) {
            return false;
        }

        // 2) если категория разрешена — сразу true
        let cat = match category {
            "read" => CmdCategory::READ,
            "write" => CmdCategory::WRITE,
            "admin" => CmdCategory::ADMIN,
            _ => CmdCategory::empty(),
        };

        // сначала категория
        if self.allowed_categories.contains(cat) {
            // если есть wildcard всех команд в этой категории - сразу true
            return true;
        }

        // 3) иначе проверяем разрешённые конкретные команды
        self.allowed_commands.contains(&cmd)
    }

    pub fn check_key(&self, key: &str) -> bool {
        self.enabled && self.key_patterns.is_match(key)
    }

    // Проверка Pub/Sub-канала по шаблонам.
    pub fn check_channel(&self, channel: &str) -> bool {
        self.enabled && self.channel_patterns.is_match(channel)
    }
}

impl Acl {
    /// Установить/обновить пользователя с набором ACL-правил.
    pub fn acl_setuser(&self, username: &str, rules: &[&str]) -> Result<(), AclError> {
        let mut users = self.users.write().unwrap();
        let user = users
            .entry(username.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(AclUser::new(username).unwrap())));

        let mut user = user.write().unwrap();

        // очищаем прежние ACL (кроме username)
        user.enabled = false;
        user.password_hashes.clear();
        user.allowed_categories = CmdCategory::empty();
        user.allowed_commands.clear();
        user.denied_commands.clear();
        user.raw_key_patterns.clear();
        user.raw_channel_patterns.clear();
        user.key_patterns = GlobSetBuilder::new().build().unwrap();
        user.channel_patterns = GlobSetBuilder::new().build().unwrap();

        for rule in rules {
            self.apply_rule(&mut user, rule)?;
        }
        Ok(())
    }

    fn apply_rule(&self, user: &mut AclUser, rule: &str) -> Result<(), AclError> {
        // включение/отключение
        if rule == "on" {
            user.enabled = true;
            return Ok(());
        }
        if rule == "off" {
            user.enabled = false;
            return Ok(());
        }
        let first = rule.chars().next().unwrap();
        match first {
            '>' => {
                user.password_hashes.push(rule[1..].to_string());
            }
            '+' => {
                if rule.starts_with("+@") {
                    match &rule[2..] {
                        "read" => user.allowed_categories |= CmdCategory::READ,
                        "write" => user.allowed_categories |= CmdCategory::WRITE,
                        "admin" => user.allowed_categories |= CmdCategory::ADMIN,
                        "all" => {
                            user.allowed_categories =
                                CmdCategory::READ | CmdCategory::WRITE | CmdCategory::ADMIN;
                        }
                        other => return Err(AclError::InvalidAclRule(other.into())),
                    }
                } else {
                    user.allowed_commands.insert(rule[1..].to_lowercase());
                }
            }
            '-' => {
                if rule.starts_with("-@") {
                    // запрет целой категории (опционально)
                    match &rule[2..] {
                        "read" => user.allowed_categories.remove(CmdCategory::READ),
                        "write" => user.allowed_categories.remove(CmdCategory::WRITE),
                        "admin" => user.allowed_categories.remove(CmdCategory::ADMIN),
                        other => return Err(AclError::InvalidAclRule(other.into())),
                    }
                } else {
                    user.denied_commands.insert(rule[1..].to_lowercase());
                }
            }
            '~' => {
                let g = Glob::new(&rule[1..]).map_err(|_| AclError::InvalidAclRule(rule.into()))?;
                user.raw_key_patterns.push(g);
                user.rebuild_key_patterns()?;
            }
            '&' => {
                let g = Glob::new(&rule[1..]).map_err(|_| AclError::InvalidAclRule(rule.into()))?;
                user.raw_channel_patterns.push(g);
                user.rebuild_channel_patterns()?;
            }
            _ => return Err(AclError::InvalidAclRule(rule.into())),
        }
        Ok(())
    }

    /// Получаем копию AclUser.
    pub fn acl_getuser(&self, username: &str) -> Option<AclUser> {
        self.users
            .read()
            .unwrap()
            .get(username)
            .map(|u| u.read().unwrap().clone())
    }

    /// Удалить пользователя.
    pub fn acl_deluser(&self, username: &str) -> Result<(), AclError> {
        let removed = self.users.write().unwrap().remove(username);
        if removed.is_some() {
            Ok(())
        } else {
            Err(AclError::UserNotFound)
        }
    }

    pub fn acl_users(&self) -> Vec<String> {
        self.users.read().unwrap().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверяет, что пользователь по умолчанию (`AclUser::new`) включён,
    /// имеет доступ ко всем категориям, командам, ключам и каналам.
    #[test]
    fn default_user_allows_everything() {
        // AlcUser::new даёт пользователя с enabled = true, все категории, шаблоны "*".
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
            "on",     // включаем
            "+@read", // все команды категории read
            "+get",   // разрешаем get вне зависимости от категории
            "-del",   // запрещаем del
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
            "on", "~data:*", // ключи, начинающиеся с data:
            "&chan?",  // каналы chan1, chan2 …
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
            "off",   // выключаем полностью
            "+@all", // пусть будет, но disabled всё равно перекроет
            "~*",    // шаблон «всё»
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
}
