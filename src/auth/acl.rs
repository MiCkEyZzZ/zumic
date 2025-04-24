use globset::{Glob, GlobSet, GlobSetBuilder};

use std::collections::{HashMap, HashSet};
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

/// Конфигурация пользователя ACL.
///
/// Содержит информацию об имени пользователя, состоянии,
/// хэшах паролей, разрешённых категориях и командах, а также
/// шаблонах ключей и каналов.
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
    /// "Сырые" шаблоны каналов в виде `Glob`.
    raw_channel_patterns: Vec<Glob>,
    /// Скомпилированный набор шаблонов каналов.
    pub channel_patterns: GlobSet,
}

/// Основная структура для управления ACL (Access Control List).
///
/// Содержит набор пользователей ACL, доступных для чтения и модификации.
#[derive(Default, Debug)]
pub struct Acl {
    users: RwLock<HashMap<String, Arc<RwLock<AclUser>>>>,
}

impl AclUser {
    /// Создает нового пользователя ACL с заданным именем.
    ///
    /// По умолчанию пользователь включён, не имеет установленных паролей,
    /// разрешены все категории команд, а шаблоны ключей и каналов разрешают всё
    /// (эквивалентно правилам `+@all` и `~*`).
    ///
    /// # Аргументы
    ///
    /// * `username` - имя пользователя.
    ///
    /// # Возвращаемое значение
    ///
    /// Результат, содержащий нового `AclUser` или ошибку `AclError` в случае неудачи.
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
        // Разрешаем все ключи по умолчанию.
        user.raw_key_patterns.push(Glob::new("*").unwrap());
        user.rebuild_key_patterns()?;
        // Разрешаем все каналы по умолчанию.
        user.raw_channel_patterns.push(Glob::new("*").unwrap());
        user.rebuild_channel_patterns()?;
        Ok(user)
    }

    /// Перестраивает компиляцию шаблонов для ключей.
    ///
    /// Использует список "сырых" шаблонов `raw_key_patterns` для создания
    /// скомпилированного набора `GlobSet`. Возвращает ошибку `AclError`, если
    /// компиляция не удалась.
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

    /// Перестраивает компиляцию шаблонов для каналов.
    ///
    /// Использует список "сырых" шаблонов `raw_channel_patterns` для создания
    /// скомпилированного набора `GlobSet`. Возвращает ошибку `AclError`, если
    /// компиляция не удалась.
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

    /// Проверяет, имеет ли пользователь право выполнить команду.
    ///
    /// Сначала проверяется, включён ли пользователь и нет ли явного запрета для команды.
    /// Затем проверяется, разрешена ли категория команды. Если да, команда разрешается.
    /// Иначе выполняется проверка на наличие команды в списке разрешённых.
    ///
    /// # Аргументы
    ///
    /// * `category` - категория команды в виде строки (например, `"read"`, `"write"`, `"admin"`).
    /// * `command` - имя команды.
    ///
    /// # Возвращаемое значение
    ///
    /// `true`, если команда разрешена для данного пользователя, иначе `false`.
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

    /// Проверяет, разрешён ли доступ к заданному ключу.
    ///
    /// Использует скомпилированный набор шаблонов `key_patterns`.
    ///
    /// # Аргументы
    ///
    /// * `key` - ключ, который нужно проверить.
    ///
    /// # Возвращаемое значение
    ///
    /// `true`, если ключ соответствует хотя бы одному из шаблонов и пользователь включён.
    pub fn check_key(&self, key: &str) -> bool {
        self.enabled && self.key_patterns.is_match(key)
    }

    /// Проверяет доступность Pub/Sub-канала на основе заданных шаблонов.
    ///
    /// Использует скомпилированный набор шаблонов `channel_patterns`.
    ///
    /// # Аргументы
    ///
    /// * `channel` - название канала, которое нужно проверить.
    ///
    /// # Возвращаемое значение
    ///
    /// `true`, если канал соответствует хотя бы одному шаблону и пользователь включён.
    pub fn check_channel(&self, channel: &str) -> bool {
        self.enabled && self.channel_patterns.is_match(channel)
    }
}

impl Acl {
    /// Устанавливает или обновляет пользователя с набором правил ACL.
    ///
    /// При этом происходит очистка предыдущих правил (кроме имени пользователя),
    /// после чего к пользователю применяются переданные правила.
    ///
    /// # Аргументы
    ///
    /// * `username` - имя пользователя для которого применяется набор правил.
    /// * `rules` - срез строк, представляющих ACL-правила (например, `"on"`, `"+@read"`, `"-del"`).
    ///
    /// # Возвращаемое значение
    ///
    /// Результат выполнения операции. В случае ошибки возвращается `AclError`.
    pub fn acl_setuser(&self, username: &str, rules: &[&str]) -> Result<(), AclError> {
        let mut users = self.users.write().unwrap();
        let user = users
            .entry(username.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(AclUser::new(username).unwrap())));

        let mut user = user.write().unwrap();

        // Очищаем прежние настройки (за исключением имени пользователя).
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

    /// Применяет отдельное правило ACL к пользователю.
    ///
    /// Обрабатываются разные типы правил:
    /// - `"on"` и `"off"` — включение/выключение пользователя;
    /// - `"+"` и `"-"` для разрешения или запрета категорий и команд;
    /// - `"~"` и `"&"` для задания шаблонов ключей и каналов.
    ///
    /// # Аргументы
    ///
    /// * `user` - изменяемый пользователь ACL.
    /// * `rule` - правило в виде строки.
    ///
    /// # Возвращаемое значение
    ///
    /// Результат выполнения операции. При ошибке возвращается `AclError::InvalidAclRule`.
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

    /// Возвращает копию данных пользователя ACL по его имени.
    ///
    /// # Аргументы
    ///
    /// * `username` - имя пользователя.
    ///
    /// # Возвращаемое значение
    ///
    /// `Some(AclUser)` если пользователь найден, иначе `None`.
    pub fn acl_getuser(&self, username: &str) -> Option<AclUser> {
        self.users
            .read()
            .unwrap()
            .get(username)
            .map(|u| u.read().unwrap().clone())
    }

    /// Удаляет пользователя ACL по его имени.
    ///
    /// # Аргументы
    ///
    /// * `username` - имя пользователя, которого необходимо удалить.
    ///
    /// # Возвращаемое значение
    ///
    /// `Ok(())` если пользователь успешно удалён, иначе ошибка `AclError::UserNotFound`.
    pub fn acl_deluser(&self, username: &str) -> Result<(), AclError> {
        let removed = self.users.write().unwrap().remove(username);
        if removed.is_some() {
            Ok(())
        } else {
            Err(AclError::UserNotFound)
        }
    }

    /// Возвращает список имен всех зарегистрированных пользователей ACL.
    ///
    /// # Возвращаемое значение
    ///
    /// Вектор строк с именами пользователей.
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
}
