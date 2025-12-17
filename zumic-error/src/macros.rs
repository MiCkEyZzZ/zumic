/// Немедленно возвращает ошибку (аналогично `anyhow::bail!`).
///
/// Макрос возвращает `Err(StackError)` из текущей функции. Поддерживает три
/// формы:
/// - `bail!(err)` — принимает уже готовый тип ошибки или
///   `StackError`-совместимый тип;
/// - `bail!(code, "msg")` — создаёт `GenericError` с кодом и сообщением;
/// - `bail!(code, "fmt {}", arg)` — форматирует сообщение.
///
/// Пример:
///
/// ```ignore
/// use zumic_error::{bail, StatusCode};
///
/// fn validate_key(key: &str) -> Result<(), crate::StackError> {
///     if key.is_empty() {
///         bail!(StatusCode::InvalidKey, "Key cannot be empty");
///     }
///     if key.len() > 1024 {
///         bail!(StatusCode::InvalidKey, "Key too long: {} bytes", key.len());
///     }
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! bail {
    ($err:expr) => {
        return Err($crate::StackError::from($err))
    };
    ($code:expr, $msg:expr) => {
        return Err($crate::StackError::new(
            $crate::types::GenericError::new($code, $msg)
        ))
    };
    ($code:expr, $fmt:expr, $($arg:tt)*) => {
        return Err($crate::StackError::new(
            $crate::types::GenericError::new($code, format!($fmt, $($arg)*))
        ))
    };
}

/// Проверяет условие и вызывает `bail!`, если условие ложно.
///
/// Формы аналогичны `bail!`:
/// - `ensure!(cond, err)` — если `cond` ложно, выполняется `bail!(err)`.
/// - `ensure!(cond, code, "msg")` — если `cond` ложно, выполняется `bail!(code,
///   "msg")`.
/// - `ensure!(cond, code, "fmt {}", arg)` — форматированная форма.
///
/// Пример:
///
/// ```ignore
/// use zumic_error::{ensure, StatusCode};
///
/// fn process(value: i64) -> Result<(), crate::StackError> {
///     ensure!(value >= 0, StatusCode::InvalidArgs, "Value must be non-negative");
///     ensure!(value < 1000, StatusCode::InvalidArgs, "Value too large: {}", value);
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $err:expr) => {
        if !($cond) {
            $crate::bail!($err);
        }
    };
    ($cond:expr, $code:expr, $msg:expr) => {
        if !($cond) {
            $crate::bail!($code, $msg);
        }
    };
    ($cond:expr, $code:expr, $fmt:expr, $($arg:tt)*) => {
        if !($cond) {
            $crate::bail!($code, $fmt, $($arg)*);
        }
    };
}

/// Добавляет контекст к `Result`.
///
/// Если аргумент — `Ok(val)`, возвращает `Ok(val)`. Если `Err(e)`, преобразует
/// `e` в `StackError` и добавляет указанный контекст (через
/// `StackError::context`).
///
/// Пример:
///
/// ```ignore
/// use zumic_error::context;
///
/// fn read_config() -> Result<Config, crate::StackError> {
///     let data = std::fs::read_to_string("config.toml")
///         .context("Failed to read config file")?;
///     // ...
///     # Ok(serde_json::from_str(&data).unwrap())
/// }
/// ```
#[macro_export]
macro_rules! context {
    ($result:expr, $msg:expr) => {
        match $result {
            Ok(val) => Ok(val),
            Err(e) => Err($crate::StackError::from(e).context($msg)),
        }
    };
    ($result:expr, $fmt:expr, $($arg:tt)*) => {
        match $result {
            Ok(val) => Ok(val),
            Err(e) => Err($crate::StackError::from(e).context(format!($fmt, $($arg)*))),
        }
    };
}

/// Трейт-расширение для `Result`, добавляющее удобные методы контекстирования.
///
/// Позволяет вызывать `.context(...)` и `.with_context(...)` на результатах,
/// превращая ошибку в [`StackError`] и приклеивая к ней контекст.
pub trait ResultExt<T> {
    /// Добавляет контекст к ошибке: если `self` — `Err`, оборачивает ошибку в
    /// `StackError` и добавляет указанный контекст.
    fn context<C>(
        self,
        ctx: C,
    ) -> Result<T, crate::StackError>
    where
        C: Into<String>;

    /// Добавляет ленивый контекст (вызывается только в случае ошибки).
    ///
    /// Полезно, если формирование строки контекста дорогостоящее.
    fn with_context<C, F>(
        self,
        f: F,
    ) -> Result<T, crate::StackError>
    where
        C: Into<String>,
        F: FnOnce() -> C;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: Into<crate::StackError>,
{
    #[track_caller]
    fn context<C>(
        self,
        ctx: C,
    ) -> Result<T, crate::StackError>
    where
        C: Into<String>,
    {
        self.map_err(|e| e.into().context(ctx))
    }

    #[track_caller]
    fn with_context<C, F>(
        self,
        f: F,
    ) -> Result<T, crate::StackError>
    where
        C: Into<String>,
        F: FnOnce() -> C,
    {
        self.map_err(|e| e.into().context(f()))
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GenericError, StatusCode, ZumicResult};

    #[test]
    fn test_bail_simple() {
        fn example() -> ZumicResult<()> {
            bail!(GenericError::new(StatusCode::InvalidArgs, "test error"));
        }

        let result = example();
        assert!(result.is_err());
    }

    #[test]
    fn test_bail_with_format() {
        fn example(value: i32) -> ZumicResult<()> {
            bail!(StatusCode::InvalidArgs, "Invalid value: {}", value);
        }

        let result = example(42);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid value: 42"));
    }

    #[test]
    fn test_ensure() {
        fn validate(x: i32) -> ZumicResult<()> {
            ensure!(x > 0, StatusCode::InvalidArgs, "Value must be positive");
            ensure!(x < 100, StatusCode::InvalidArgs, "Value too large: {}", x);
            Ok(())
        }

        assert!(validate(50).is_ok());
        assert!(validate(-1).is_err());
        assert!(validate(150).is_err());
    }

    #[test]
    fn test_result_ext() {
        fn inner() -> Result<(), GenericError> {
            Err(GenericError::new(StatusCode::Internal, "inner error"))
        }

        fn outer() -> ZumicResult<()> {
            inner().context("outer context")?;
            Ok(())
        }

        let result = outer();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.contexts().len(), 1);
        assert_eq!(err.contexts()[0].message, "outer context");
    }

    #[test]
    fn test_with_context_lazy() {
        fn expensive_context() -> String {
            "expensive context".to_string()
        }

        fn example(success: bool) -> ZumicResult<()> {
            let result: Result<(), GenericError> = if success {
                Ok(())
            } else {
                Err(GenericError::new(StatusCode::Internal, "error"))
            };

            result.with_context(|| expensive_context())?;
            Ok(())
        }

        // При успехе expensive_context не вызывается
        assert!(example(true).is_ok());

        // При ошибке вызывается
        let result = example(false);
        assert!(result.is_err());
    }
}
