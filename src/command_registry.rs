use std::collections::HashMap;

use crate::db_context::DbContext;

/// Ф-я обработчик команды: получает &mut DbContext и сырые аргументы.
pub type Handler = Box<dyn Fn(&mut DbContext, &[u8]) -> Vec<u8> + Send + Sync>;

pub struct CommandRegistry {
    handlers: HashMap<String, Handler>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Регистрирует команду `name` с обработчиком `h`.
    pub fn register<F>(
        &mut self,
        name: impl Into<String>,
        h: F,
    ) where
        F: Fn(&mut DbContext, &[u8]) -> Vec<u8> + Send + Sync + 'static,
    {
        self.handlers.insert(name.into(), Box::new(h));
    }

    /// Вызывает handler для `name`. Паникует, если команда не найдена.
    pub fn call(
        &self,
        name: &str,
        ctx: &mut DbContext,
        data: &[u8],
    ) -> Vec<u8> {
        let h = self
            .handlers
            .get(name)
            .unwrap_or_else(|| panic!("Unknown command: {name}"));
        h(ctx, data)
    }

    /// Регистрирует все стандартные команды ZSP.
    pub fn register_builtin_commands(&mut self) {
        use crate::{Sds, Value};

        // === PING ===
        self.register("PING", |_ctx, _| b"+PONG\r\n".to_vec());

        // === ECHO ===
        self.register("ECHO", |_ctx, data| {
            let val = Value::from_bytes(data).unwrap_or(Value::Null);
            val.to_bytes()
        });

        // === SADD ===
        self.register("SADD", |ctx, data| {
            let args = Value::from_bytes(data).unwrap();
            let arr = args.as_array().unwrap();
            if arr.len() < 2 {
                return b"-ERR wrong number of arguments for 'SADD'\r\n".to_vec();
            }
            let key = Sds::from(arr[0].as_str().unwrap().as_bytes());
            let members: Vec<Sds> = arr[1..]
                .iter()
                .map(|v| Sds::from(v.as_str().unwrap().as_bytes()))
                .collect();
            let added = ctx.sadd(&key, &members).unwrap_or(0);
            format!(":{}\r\n", added).into_bytes()
        });

        // === SMEMBERS ===
        self.register("SMEMBERS", |ctx, data| {
            let args = Value::from_bytes(data).unwrap();
            let arr = args.as_array().unwrap();
            if arr.len() != 1 {
                return b"-ERR wrong number of arguments for 'SMEMBERS'\r\n".to_vec();
            }
            let key = Sds::from(arr[0].as_str().unwrap().as_bytes());
            let members = ctx.smembers(&key).unwrap_or_default();
            let values: Vec<Value> = members.into_iter().map(Value::Str).collect();
            Value::Array(values).to_bytes()
        });

        // === SCARD ===
        self.register("SCARD", |ctx, data| {
            let args = Value::from_bytes(data).unwrap();
            let arr = args.as_array().unwrap();
            if arr.len() != 1 {
                return b"-ERR wrong number of arguments for 'SCARD'\r\n".to_vec();
            }
            let key = Sds::from(arr[0].as_str().unwrap().as_bytes());
            let count = ctx.scard(&key).unwrap_or(0);
            format!(":{}\r\n", count).into_bytes()
        });

        // === SREM ===
        self.register("SREM", |ctx, data| {
            let args = Value::from_bytes(data).unwrap();
            let arr = args.as_array().unwrap();
            if arr.len() < 2 {
                return b"-ERR wrong number of arguments for 'SREM'\r\n".to_vec();
            }
            let key = Sds::from(arr[0].as_str().unwrap().as_bytes());
            let members: Vec<Sds> = arr[1..]
                .iter()
                .map(|v| Sds::from(v.as_str().unwrap().as_bytes()))
                .collect();
            let removed = ctx.srem(&key, &members).unwrap_or(0);
            format!(":{}\r\n", removed).into_bytes()
        });

        // === SISMEMBER ===
        self.register("SISMEMBER", |ctx, data| {
            let args = Value::from_bytes(data).unwrap();
            let arr = args.as_array().unwrap();
            if arr.len() != 2 {
                return b"-ERR wrong number of arguments for 'SISMEMBER'\r\n".to_vec();
            }
            let key = Sds::from(arr[0].as_str().unwrap().as_bytes());
            let member = Sds::from(arr[1].as_str().unwrap().as_bytes());
            let exists = ctx.sismember(&key, &member).unwrap_or(false);
            format!(":{}\r\n", if exists { 1 } else { 0 }).into_bytes()
        });

        // === SRANDMEMBER ===
        self.register("SRANDMEMBER", |ctx, data| {
            let args = Value::from_bytes(data).unwrap();
            let arr = args.as_array().unwrap();
            if arr.is_empty() {
                return b"-ERR wrong number of arguments for 'SRANDMEMBER'\r\n".to_vec();
            }

            let key = Sds::from(arr[0].as_str().unwrap().as_bytes());
            let count = if arr.len() > 1 {
                arr[1].as_int().unwrap_or(1)
            } else {
                1
            };

            let members = ctx.srandmember(&key, count as isize).unwrap_or_default();
            let values: Vec<Value> = members.into_iter().map(Value::Str).collect();
            Value::Array(values).to_bytes()
        });

        // === SPOP ===
        self.register("SPOP", |ctx, data| {
            let args = Value::from_bytes(data).unwrap();
            let arr = args.as_array().unwrap();
            if arr.is_empty() {
                return b"-ERR wrong number of arguments for 'SPOP'\r\n".to_vec();
            }

            let key = Sds::from(arr[0].as_str().unwrap().as_bytes());
            let count = if arr.len() > 1 {
                arr[1].as_int().unwrap_or(1)
            } else {
                1
            };

            let popped = ctx.spop(&key, count as isize).unwrap_or_default();
            let values: Vec<Value> = popped.into_iter().map(Value::Str).collect();
            Value::Array(values).to_bytes()
        });
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для CommandRegistry, ConnectionRegistry
////////////////////////////////////////////////////////////////////////////////

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Sds, Value};

    /// Тест проверяет базовую регистрацию и вызов простой команды (`ping`),
    /// возвращающей предопределённый результат.
    #[test]
    fn test_register_and_call_simple_command() {
        let mut registry = CommandRegistry::new();
        let mut ctx = DbContext::new_inmemory();

        registry.register("ping", |_ctx, _data| b"pong".to_vec());

        let result = registry.call("ping", &mut ctx, b"");
        assert_eq!(result, b"pong");
    }

    /// Тест проверяет, что команда может получать входные данные (`echo`)
    /// и возвращать результат, включающий эти данные.
    #[test]
    fn test_command_receives_data() {
        let mut registry = CommandRegistry::new();
        let mut ctx = DbContext::new_inmemory();

        registry.register("echo", |_ctx, data| {
            let mut out = b"echo: ".to_vec();
            out.extend_from_slice(data);
            out
        });

        let result = registry.call("echo", &mut ctx, b"hello");
        assert_eq!(result, b"echo: hello");
    }

    /// Тест проверяет, что команды могут взаимодействовать с контекстом базы
    /// данных (`DbContext`): одна команда сохраняет значение, другая —
    /// извлекает его.
    #[test]
    fn test_command_can_use_db_context() {
        let mut registry = CommandRegistry::new();
        let mut ctx = DbContext::new_inmemory();

        registry.register("store", |ctx, data| {
            let key = Sds::from(b"mykey".as_ref());
            let value = Value::from_bytes(data).unwrap();
            ctx.set(key, value).unwrap();
            b"OK".to_vec()
        });

        registry.register("load", |ctx, _| {
            let key = Sds::from(b"mykey".as_ref());
            if let Ok(Some(v)) = ctx.get(key) {
                v.to_bytes()
            } else {
                b"NOT_FOUND".to_vec()
            }
        });

        let serialized = Value::Str(Sds::from(b"abc123".as_ref())).to_bytes();
        registry.call("store", &mut ctx, &serialized);

        let result = registry.call("load", &mut ctx, b"");

        let value = Value::from_bytes(&result).unwrap();
        assert_eq!(value, Value::Str(Sds::from(b"abc123".as_ref())));
    }

    /// Тест проверяет, что при попытке вызвать неизвестную команду происходит
    /// паника с ожидаемым сообщением об ошибке.
    #[test]
    #[should_panic(expected = "Unknown command: missing")]
    fn test_call_unknown_command_panics() {
        let registry = CommandRegistry::new();
        let mut ctx = DbContext::new_inmemory();
        registry.call("missing", &mut ctx, b"");
    }
}
