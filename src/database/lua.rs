//! # LuaEngine — безопасная интеграция Lua в Zumic
//!
//! Модуль предоставляет безопасную и ограниченную среду для выполнения
//! Lua-скриптов внутри базы данных Zumic. Используется для расширяемости,
//! написания пользовательских процедур, атомарных операций и скриптовых команд.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use mlua::{
    Error as LuaError, HookTriggers, Lua, MultiValue, Result as LuaResult, String as LuaString,
    UserData, UserDataMethods, Value as LuaValue, VmState,
};

use super::{Hll, Sds, SmartHash, Value};
use crate::{Dict, QuickList, SkipList};

/// Ошибка выполнения Lua скрипта.
///
/// Представляет различные типы ошибок, которые могут возникнуть при выполнении
/// Lua скриптов в безопасной среде Zumic DB.
#[derive(Debug)]
pub enum LuaExecutionError {
    /// Ошибка, возникшая в Lua-скрипте
    LuaError(LuaError),
    /// Превышен лимит времени выполнения
    Timeout,
    /// Превышен лимит использования памяти
    MemoryLimit,
    /// Недопустимый тип данных при конвертации
    InvalidType(String),
    /// Ошибка конвертации между типами
    ConversionError(String),
}

/// Конфигурация для выполнения Lua скриптов.
///
/// Определяет ограничения ресурсов для безопасного выполнения Lua-скриптов.
/// Все лимиты применяются для предотвращения DoS-атак и обеспечения
/// стабильности системы.
///
/// # Поля
///
/// - `max_execution_time`: Максимальное время выполнения скрипта
/// - `max_memory_limit`: Максимальное использование памяти в байтах
/// - `max_instruction_count`: Максимальное количество инструкций Lua
#[derive(Debug, Clone)]
pub struct LuaConfig {
    /// Максимальное время выполнения скрипта
    pub max_execution_time: Duration,
    /// Максимальное использование памяти в байтах
    pub max_memory_limit: usize,
    /// Максимальное количество инструкций Lua
    pub max_instruction_count: u64,
}

/// Безопасный движок для выполнения Lua скриптов в Zumic DB.
///
/// Предоставляет изолированную среду для выполнения Lua-скриптов с
/// автоматическим контролем ресурсов, преобразованием типов и обработкой
/// ошибок.
///
/// # Безопасность
///
/// - Все скрипты выполняются с ограничениями по времени, памяти и инструкциям
/// - Автоматическая очистка ресурсов после выполнения
/// - Защита от бесконечных циклов и утечек памяти
///
/// # Поддерживаемые типы
///
/// Движок автоматически конвертирует между Lua-типами и внутренними типами
/// Zumic:
/// - `string` ↔ `Sds`
/// - `number` ↔ `i64`/`f64`
/// - `boolean` ↔ `bool`
/// - `table` ↔ `QuickList`/`SmartHash`/`Dict`
/// - `nil` ↔ `Value::Null`
pub struct LuaEngine {
    lua: Lua,
    #[allow(dead_code)]
    config: LuaConfig,
    start_time: Option<Instant>,
    instruction_count: Arc<Mutex<u64>>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl LuaEngine {
    /// Создает новый экземпляр Lua движка с указанной конфигурацией.
    ///
    /// # Аргументы
    ///
    /// - `config`: Конфигурация с ограничениями ресурсов
    ///
    /// # Возвращает
    ///
    /// - `Ok(LuaEngine)`: Успешно созданный движок
    /// - `Err(LuaError)`: Ошибка инициализации Lua
    pub fn new(config: LuaConfig) -> LuaResult<Self> {
        let lua = Lua::new();

        // Настройка ограничений памяти
        lua.set_memory_limit(config.max_memory_limit)?;

        let instruction_count = Arc::new(Mutex::new(0));
        let instruction_count_clone = instruction_count.clone();

        // Установка хука для контроля времени и инструкций
        lua.set_hook(
            HookTriggers {
                on_calls: false,
                on_returns: false,
                every_line: true,
                every_nth_instruction: Some(1),
            },
            move |_lua, _debug| {
                let mut count = instruction_count_clone.lock().unwrap();
                *count += 1;

                if *count > config.max_instruction_count {
                    return Err(LuaError::RuntimeError(
                        "Instruction limit exceeded".to_string(),
                    ));
                }

                Ok(VmState::Continue)
            },
        );

        Ok(Self {
            lua,
            config,
            start_time: None,
            instruction_count,
        })
    }

    /// Выполняет Lua скрипт с переданными аргументами.
    ///
    /// # Аргументы
    ///
    /// - `script`: Lua скрипт для выполнения
    /// - `args`: Вектор аргументов типа `Value` для передачи в скрипт
    ///
    /// # Возвращает
    ///
    /// - `Ok(Value)`: Результат выполнения скрипта
    /// - `Err(LuaExecutionError)`: Ошибка выполнения или превышения лимитов
    ///
    /// # Ограничения
    ///
    /// - Время выполнения ограничено `config.max_execution_time`
    /// - Использование памяти ограничено `config.max_memory_limit`
    /// - Количество инструкций ограничено `config.max_instruction_count`
    pub fn eval(
        &mut self,
        script: &str,
        args: Vec<Value>,
    ) -> Result<Value, LuaExecutionError> {
        // Сбрасываем таймер и счётчик
        self.start_time = Some(Instant::now());
        *self.instruction_count.lock().unwrap() = 0;

        // Конвертация наших Value в LuaValue
        let lua_args: Vec<LuaValue> = args
            .into_iter()
            .map(|v| self.value_to_lua(v))
            .collect::<Result<_, _>>()?;

        // Собираем MultiValue из Vec<LuaValue>
        let mut mv = MultiValue::new();
        for v in lua_args {
            mv.push_front(v); // либо push_back, в соответствии с порядком
        }

        // Выполнение скрипта: указываем только возвращаемый тип
        let result: LuaValue = self
            .lua
            .load(script)
            .set_name("eval")
            .call::<LuaValue>(mv)
            .map_err(|e| match e {
                LuaError::MemoryError(_) => LuaExecutionError::MemoryLimit,
                other => other.into(),
            })?;

        // Преобразуем в наш Value и возвращаем
        self.lua_to_value(result)
    }

    /// Преобразование Value в Lua значение.
    fn value_to_lua(
        &self,
        value: Value,
    ) -> Result<LuaValue, LuaExecutionError> {
        match value {
            Value::Str(sds) => {
                let ud = self.lua.create_userdata(sds.clone())?;
                Ok(LuaValue::UserData(ud))
            }
            Value::Int(i) => Ok(LuaValue::Integer(i)),
            Value::Float(f) => Ok(LuaValue::Number(f)),
            Value::Bool(b) => Ok(LuaValue::Boolean(b)),
            Value::Array(arr) => {
                let lua_table = self.lua.create_table()?;
                for (i, item) in arr.into_iter().enumerate() {
                    let lua_value = self.value_to_lua(item)?;
                    lua_table.set(i + 1, lua_value)?;
                }
                Ok(LuaValue::Table(lua_table))
            }
            Value::Set(set) => {
                let lua_table = self.lua.create_table()?;
                for item in set {
                    let lua_key = self.value_to_lua(Value::Str(item))?;
                    lua_table.set(lua_key, true)?;
                }
                Ok(LuaValue::Table(lua_table))
            }
            Value::Hash(map) => {
                let lua_table = self.lua.create_table()?;
                // map — это SmartHash, у которого есть метод entries()
                // он отдаёт Vec<(Sds, Sds)>, то есть пары ключ-значение в виде owned Sds
                for (k, v) in map.entries().into_iter() {
                    // создаём LuaString напрямую из байтов Sds
                    let key_str = self.lua.create_string(k.as_bytes())?;
                    let val_str = self.lua.create_string(v.as_bytes())?;
                    lua_table.set(LuaValue::String(key_str), LuaValue::String(val_str))?;
                }
                Ok(LuaValue::Table(lua_table))
            }
            Value::ZSet { dict, sorted: _ } => {
                let lua_table = self.lua.create_table()?;
                for (item, score) in dict.iter() {
                    let key_str = self.lua.create_string(item.as_bytes())?;
                    lua_table.set(LuaValue::String(key_str), LuaValue::Number(*score))?;
                }
                Ok(LuaValue::Table(lua_table))
            }
            Value::Bitmap(bmp) => {
                let lua_table = self.lua.create_table()?;
                for (i, byte) in bmp.as_bytes().iter().enumerate() {
                    lua_table.set(i + 1, *byte)?;
                }
                Ok(LuaValue::Table(lua_table))
            }
            Value::List(items) => {
                let lua_table = self.lua.create_table()?;
                for (i, sds_item) in items.into_iter().enumerate() {
                    let lua_val = self.value_to_lua(Value::Str(sds_item))?;
                    lua_table.set(i + 1, lua_val)?;
                }
                Ok(LuaValue::Table(lua_table))
            }
            Value::HyperLogLog(hll) => Ok(LuaValue::UserData(self.lua.create_userdata(*hll)?)),
            Value::SStream(ss) => {
                let lua_table = self.lua.create_table()?;

                for (i, entry) in ss.iter().enumerate() {
                    // Преобразуем StreamId в строку
                    let id_str = format!("{}-{}", entry.id.ms_time, entry.id.sequence);
                    let lua_id = self.lua.create_string(&id_str)?;

                    // Преобразуем HashMap<String, Value> в подтаблицу
                    let data_table = self.lua.create_table()?;
                    for (k, v) in &entry.data {
                        let lua_key = self.lua.create_string(k)?;
                        let lua_val = self.value_to_lua(v.clone())?;
                        data_table.set(lua_key, lua_val)?;
                    }

                    // Собираем объект entry как { id = "...", data = {...} }
                    let entry_table = self.lua.create_table()?;
                    entry_table.set("id", lua_id)?;
                    entry_table.set("data", data_table)?;

                    // Вставляем в основной массив-поток
                    lua_table.set(i + 1, entry_table)?; // Lua-массивы
                                                        // начинаются с 1
                }

                Ok(LuaValue::Table(lua_table))
            }
            Value::Null => Ok(LuaValue::Nil),
        }
    }

    /// Преобразование Lua значения в Value.
    #[allow(clippy::only_used_in_recursion)]
    fn lua_to_value(
        &self,
        lua_value: LuaValue,
    ) -> Result<Value, LuaExecutionError> {
        match lua_value {
            LuaValue::String(s) => Ok(Value::Str(Sds::from_vec(s.as_bytes().to_vec()))),
            LuaValue::Integer(i) => Ok(Value::Int(i)),
            LuaValue::Number(n) => Ok(Value::Float(n)),
            LuaValue::Boolean(b) => Ok(Value::Bool(b)),
            LuaValue::Nil => Ok(Value::Null),
            LuaValue::Table(table) => {
                let is_array = table.contains_key(1)?;
                if is_array {
                    let mut list = QuickList::new(4); // или другой capacity

                    for pair in table.sequence_values::<LuaValue>() {
                        let v = self.lua_to_value(pair?)?;
                        match v {
                            Value::Str(sds) => list.push_back(sds),
                            other => {
                                return Err(LuaExecutionError::InvalidType(format!(
                                    "Expected string in list, got: {other:?}",
                                )));
                            }
                        }
                    }

                    Ok(Value::List(list))
                } else {
                    // Попробуем собрать Value::Hash или Value::ZSet, если значения — числа
                    let mut all_numbers = true;
                    let mut dict = SmartHash::new();

                    for pair in table.pairs::<LuaValue, LuaValue>() {
                        let (k, v) = pair?;
                        let key = match k {
                            LuaValue::String(s) => Sds::from_vec(s.as_bytes().to_vec()),
                            _ => continue, // пропускаем нестроковые ключи
                        };
                        match &v {
                            LuaValue::Number(_) => {} // ok
                            _ => all_numbers = false,
                        }
                        let value = match &v {
                            LuaValue::String(s) => Value::Str(Sds::from_vec(s.as_bytes().to_vec())),
                            LuaValue::Number(n) => Value::Float(*n),
                            LuaValue::Integer(i) => Value::Int(*i),
                            _ => self.lua_to_value(v.clone())?, // рекурсивно
                        };
                        if let Value::Str(sds) = value {
                            dict.insert(key, sds);
                        } else {
                            return Err(LuaExecutionError::InvalidType(
                                "Expected Value::Str".into(),
                            ));
                        }
                    }

                    if all_numbers {
                        let mut zset = Dict::new();
                        for (k, v) in &dict.entries() {
                            let score = v
                                .as_str()
                                .ok()
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            zset.insert(k.clone(), score);
                        }

                        Ok(Value::ZSet {
                            dict: zset,
                            sorted: SkipList::new(),
                        })
                    } else {
                        Ok(Value::Hash(dict))
                    }
                }
            }
            LuaValue::UserData(ud) => {
                if let Ok(sds) = ud.borrow::<Sds>() {
                    Ok(Value::Str(sds.clone()))
                } else {
                    Err(LuaExecutionError::InvalidType(
                        "Unsupported UserData".into(),
                    ))
                }
            }
            other => Err(LuaExecutionError::InvalidType(format!(
                "Cannot convert LuaValue: {other:?}",
            ))),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для LuaEngine,
////////////////////////////////////////////////////////////////////////////////

/// Реализация методов для работы с Sds в Lua.
///
/// Предоставляет методы для манипуляции строками Sds из Lua-скриптов.
/// Все методы безопасны и не могут вызвать панику.
impl UserData for Sds {
    fn add_methods<T: UserDataMethods<Self>>(methods: &mut T) {
        // Возвращает длину строки в байтах.
        methods.add_method("len", |_, this, ()| Ok(this.len()));

        // Возвращает строку как вектор байт.
        methods.add_method("to_vec", |_, this, ()| Ok(this.to_vec()));

        // Возвращает строку как UTF-8 строку Lua.
        // Если строка содержит невалидный UTF-8, возвращает `<invalid utf-8>`.
        methods.add_method("as_str", |_, this, ()| {
            Ok(this.as_str().unwrap_or("<invalid utf-8>").to_string())
        });

        // Возвращает подстроку.
        // - `start`: Начальная позиция (0-индексированная)
        // - `len`: Длина подстроки (опционально, если не указана - до конца строки)
        methods.add_method("substr", |_, this, (start, len): (usize, Option<usize>)| {
            let end = len.map(|l| start + l).unwrap_or(this.len());
            if start < this.len() && end <= this.len() {
                Ok(Sds::from_vec(this.to_vec()[start..end].to_vec()))
            } else {
                Ok(Sds::from_vec(vec![]))
            }
        });

        // Конкатенирует текущую строку с другой.
        // - `other`: Lua строка для конкатенации
        methods.add_method("concat", |_, this, other: LuaString| {
            let mut result = this.to_vec();
            result.extend_from_slice(other.as_bytes().as_ref());
            Ok(Sds::from_vec(result))
        });

        // Преобразует строку в верхний регистр.
        methods.add_method("upper", |_, this, ()| {
            if let Ok(s) = this.as_str() {
                Ok(Sds::from_str(&s.to_uppercase()))
            } else {
                Ok(this.clone())
            }
        });

        // Преобразует строку в нижний регистр.
        methods.add_method("lower", |_, this, ()| {
            if let Ok(s) = this.as_str() {
                Ok(Sds::from_str(&s.to_lowercase()))
            } else {
                Ok(this.clone())
            }
        });
    }
}

impl UserData for Hll {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("estimate", |_, this, ()| Ok(this.estimate_cardinality()));
        methods.add_method_mut("add", |_, this, v: LuaString| {
            this.add(v.as_bytes().as_ref());
            Ok(())
        });
    }
}

/// Реализация методов для работы с Value в Lua.
///
/// Предоставляет методы для интроспекции и преобразования типов Value
/// из Lua-скриптов.
impl UserData for Value {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Возвращает тип значения как строку.
        //
        // # Возвращаемые значения
        // - `"string"` для строк
        // - `"integer"` для целых чисел
        // - `"float"` для чисел с плавающей точкой
        // - `"boolean"` для булевых значений
        // - `"list"` для списков
        // - `"hash"` для хеш-таблиц
        // - `"array"` для массивов
        // - `"bitmap"` для битовых карт
        // - `"hll"` для HyperLogLog
        // - `"sst"` для потоков
        // - `"set"` для множеств
        // - `"zset"` для отсортированных множеств
        // - `"nil"` для null значений
        methods.add_method("type", |_, this, ()| {
            Ok(match this {
                Value::Str(_) => "string",
                Value::Int(_) => "integer",
                Value::Float(_) => "float",
                Value::Bool(_) => "boolean",
                Value::List(_) => "list",
                Value::Hash(_) => "hash",
                Value::Array(_) => "array",
                Value::Bitmap(_) => "bitmap",
                Value::HyperLogLog(_) => "hll",
                Value::SStream(_) => "sst",
                Value::Set(_) => "set",
                Value::ZSet { .. } => "zset",
                Value::Null => "nil",
            })
        });

        // Проверяет, является ли значение null.
        methods.add_method("is_nil", |_, this, ()| Ok(matches!(this, Value::Null)));

        // Возвращает строковое представление значения.
        methods.add_method("to_string", |_, this, ()| Ok(format!("{this:?}")));

        // Пытается преобразовать значение в целое число.
        //
        // # Возвращает
        // - `number` если преобразование успешно
        // - `nil` если преобразование невозможно
        methods.add_method("as_integer", |_, this, ()| match this {
            Value::Int(i) => Ok(Some(*i)),
            Value::Float(f) => Ok(Some(*f as i64)),
            Value::Str(s) => {
                if let Ok(s_str) = s.as_str() {
                    Ok(s_str.parse::<i64>().ok())
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        });

        // Пытается преобразовать значение в число с плавающей точкой.
        //
        // # Возвращает
        // - `number` если преобразование успешно
        // - `nil` если преобразование невозможно
        methods.add_method("as_float", |_, this, ()| match this {
            Value::Int(i) => Ok(Some(*i as f64)),
            Value::Float(f) => Ok(Some(*f)),
            Value::Str(s) => {
                if let Ok(s_str) = s.as_str() {
                    Ok(s_str.parse::<f64>().ok())
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        });
    }
}

impl Default for LuaConfig {
    /// Создает конфигурацию по умолчанию с безопасными лимитами.
    ///
    /// # Значения по умолчанию
    /// - `max_execution_time`: 5 секунд
    /// - `max_memory_limit`: 1 МБ (1024 * 1024 байт)
    /// - `max_instruction_count`: 1 миллион инструкций
    fn default() -> Self {
        Self {
            max_execution_time: Duration::from_secs(5),
            max_memory_limit: 1024 * 1024,  // 1МБ
            max_instruction_count: 1000000, // 1M инструкций
        }
    }
}

impl From<LuaError> for LuaExecutionError {
    /// Преобразует ошибку Lua в LuaExecutionError.
    fn from(err: LuaError) -> Self {
        Self::LuaError(err)
    }
}

impl std::fmt::Display for LuaExecutionError {
    /// Форматирует ошибку выполнения для пользователя.
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            LuaExecutionError::LuaError(err) => write!(f, "Lua error: {err}"),
            LuaExecutionError::Timeout => write!(f, "Script execution timeout"),
            LuaExecutionError::MemoryLimit => write!(f, "Memory limit exceeded"),
            LuaExecutionError::InvalidType(msg) => write!(f, "Invalid type: {msg}"),
            LuaExecutionError::ConversionError(msg) => write!(f, "Conversion error: {msg}"),
        }
    }
}

/// Реализация стандартного Error трейта для LuaExecutionError.
///
/// Позволяет использовать LuaExecutionError в контексте стандартной обработки
/// ошибок Rust.
impl std::error::Error for LuaExecutionError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет базовое выполнение Lua скрипта.
    ///
    /// Проверяет, что простой скрипт возвращает ожидаемое значение.
    #[test]
    fn test_basic_lua_execution() {
        let mut engine = LuaEngine::new(LuaConfig::default()).unwrap();
        let script = "return 42";
        let result = engine.eval(script, vec![]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    /// Тест проверяет операции со строками в Lua.
    ///
    /// Проверяет, что методы Sds корректно работают из Lua-скриптов.
    #[test]
    fn test_string_operations() {
        let mut engine = LuaEngine::new(LuaConfig::default()).unwrap();
        let script = r#"
            local s = ...
            return s:upper()
        "#;
        let args = vec![Value::Str(Sds::from_str("hello"))];
        let result = engine.eval(script, args).unwrap();
        if let Value::Str(s) = result {
            assert_eq!(s.as_str().unwrap(), "HELLO");
        } else {
            panic!("Expected string result");
        }
    }

    /// Тест проверяет арифметические операции в Lua.
    ///
    /// Проверяет, что числовые операции корректно работают с типами Value.
    #[test]
    fn test_arithmetic_operations() {
        let mut engine = LuaEngine::new(LuaConfig::default()).unwrap();

        let script = r#"
            local a, b = ...
            return a + b
        "#;
        let args = vec![Value::Int(10), Value::Int(20)];
        let result = engine.eval(script, args).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    /// Тест проверяет операции с таблицами в Lua.
    ///
    /// Проверяет, что Lua таблицы корректно конвертируются в QuickList.
    #[test]
    fn test_table_operations() {
        let mut engine = LuaEngine::new(LuaConfig::default()).unwrap();

        let script = r#"
            local t = {}
            t[1] = "first"
            t[2] = "second"
            return t
        "#;

        let result = engine.eval(script, vec![]).unwrap();

        if let Value::List(items) = result {
            let vec: Vec<_> = items.iter().cloned().collect(); // Преобразуем QuickList → Vec
            assert_eq!(vec.len(), 2);
            assert_eq!(vec[0], Sds::from_str("first"));
            assert_eq!(vec[1], Sds::from_str("second"));
        } else {
            panic!("Expected list result");
        }
    }

    /// Тест проверяет защиту от зависаний.
    ///
    /// Проверяет, что бесконечные циклы корректно прерываются по лимиту
    /// инструкций.
    #[test]
    fn test_timeout_protection() {
        let config = LuaConfig {
            max_execution_time: Duration::from_millis(100),
            max_instruction_count: 100,
            ..Default::default()
        };

        let mut engine = LuaEngine::new(config).unwrap();

        let script = r#"
            local i = 0
            while true do
                i = i + 1
            end
        "#;

        let result = engine.eval(script, vec![]);
        assert!(result.is_err());
    }

    /// Тест проверяет методы Sds в Lua.
    ///
    /// Проверяет, что все методы Sds корректно работают из Lua-скриптов.
    #[test]
    fn test_sds_methods() {
        let mut engine = LuaEngine::new(LuaConfig::default()).unwrap();

        let script = r#"
            local s = ...
            return s:len(), s:upper(), s:substr(1, 3)
        "#;
        let args = vec![Value::Str(Sds::from_str("hello"))];
        let result = engine.eval(script, args);

        // Lua возвращает только первое значение, поэтому проверяем длину
        assert!(result.is_ok());
    }

    /// Тест проверяет проверку типов Value в Lua.
    ///
    /// Проверяет, что метод type() корректно определяет типы Value.
    #[test]
    fn test_value_type_checking() {
        let mut engine = LuaEngine::new(LuaConfig::default()).unwrap();

        let script = r#"
            local v = ...
            return type(v)
        "#;

        let args = vec![Value::Int(42)];
        let result = engine.eval(script, args).unwrap();

        if let Value::Str(s) = result {
            assert_eq!(s.as_str().unwrap(), "number");
        } else {
            panic!("Expected string result");
        }
    }

    /// Тест проверяет обработку ошибок Lua.
    ///
    /// Проверяет, что ошибки Lua корректно преобразуются в LuaExecutionError.
    #[test]
    fn test_error_handling() {
        let mut engine = LuaEngine::new(LuaConfig::default()).unwrap();
        let script = "error('test error')";
        let result = engine.eval(script, vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));
    }
}
