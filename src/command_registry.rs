use std::collections::HashMap;

use crate::db_context::DbContext;

/// Ф-я обработчик команды: получает &mut DbContext и сырые аргументы.
pub type Handler = Box<dyn Fn(&mut DbContext, &[u8]) -> Vec<u8> + Send + Sync>;

pub struct CommandRegistry {
    handlers: HashMap<String, Handler>,
}

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
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

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
