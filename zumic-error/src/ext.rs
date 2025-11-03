use std::{any::Any, error::Error};

use crate::StatusCode;

/// Расширение для ошибок библиотеки (object-safe).
///
/// Предоставляет вспомогательные методы для работы с ошибками:
/// - извлечение статус-кода,
/// - безопасное сообщение для клиента,
/// - детализированное сообщение для логов,
/// - формирование тегов для систем наблюдаемости (observability).
pub trait ErrorExt: Error + Send + Sync + 'static {
    /// Протокольный статус (для клиента или сетевого уровня).
    ///
    /// По умолчанию возвращает [`StatusCode::Internal`].
    fn status_code(&self) -> StatusCode {
        StatusCode::Internal
    }

    /// Возвращает ошибку как [`Any`](std::any::Any),
    /// чтобы можно было выполнить downcast к конкретному типу.
    fn as_any(&self) -> &dyn Any;

    /// Безопасное сообщение для клиента.
    ///
    /// Не содержит внутренних деталей реализации, чтобы не раскрывать
    /// чувствительные данные. Для внутренних ошибок возвращает строку
    /// `"Internal server error"`.
    fn client_message(&self) -> String {
        match self.status_code() {
            crate::status_code::StatusCode::Unknown
            | crate::status_code::StatusCode::Internal
            | crate::status_code::StatusCode::Unexpected => "Internal server error".to_string(),
            _ => self.to_string(),
        }
    }

    /// Детализированное сообщение для логов.
    ///
    /// Может содержать чувствительные данные, поэтому предназначено только
    /// для внутреннего использования (логирование, отладка).
    fn log_message(&self) -> String {
        format!("{self:?}")
    }

    /// Набор тегов для систем наблюдаемости (Prometheus, DataDog и др.).
    ///
    /// Возвращает список пар ключ–значение, используемых в метриках.
    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        vec![
            ("error_type", self.type_name()),
            ("status_code", self.status_code().to_string()),
        ]
    }

    /// Имя типа ошибки (для метрик или логирования).
    fn type_name(&self) -> String {
        std::any::type_name::<Self>()
            .split("::")
            .last()
            .unwrap_or("Unknown")
            .to_string()
    }
}

/// Обёртка для хранения любых ошибок, реализующих `ErrorExt`.
/// Удобна как единый тип ошибки в публичных API.
pub struct BoxedError {
    inner: Box<dyn ErrorExt>,
}

impl BoxedError {
    pub fn new<E: ErrorExt>(err: E) -> Self {
        Self {
            inner: Box::new(err),
        }
    }

    pub fn into_inner(self) -> Box<dyn ErrorExt> {
        self.inner
    }
}

impl std::fmt::Debug for BoxedError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        // Для отладки используем debug внутренней ошибки
        write!(f, "{:?}", self.inner)
    }
}

impl std::fmt::Display for BoxedError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl std::error::Error for BoxedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }
}

impl ErrorExt for BoxedError {
    fn status_code(&self) -> crate::StatusCode {
        self.inner.status_code()
    }

    fn as_any(&self) -> &dyn Any {
        self.inner.as_any()
    }
}

#[cfg(test)]
mod tests {
    use std::{any::Any, error::Error, fmt};

    use super::*;

    // Вспомогательный тип ошибки без переопределения status_code (использует
    // default = Internal).
    #[derive(Debug)]
    struct DefaultError(pub &'static str);

    impl fmt::Display for DefaultError {
        fn fmt(
            &self,
            f: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            write!(f, "DefaultError: {}", self.0)
        }
    }

    impl Error for DefaultError {}

    impl ErrorExt for DefaultError {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // Вспомогательный тип ошибки, возвращающий конкретный StatusCode::NotFound.
    #[derive(Debug)]
    struct NotFoundError(pub &'static str);

    impl fmt::Display for NotFoundError {
        fn fmt(
            &self,
            f: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            write!(f, "NotFound: {}", self.0)
        }
    }

    impl Error for NotFoundError {}

    impl ErrorExt for NotFoundError {
        fn status_code(&self) -> StatusCode {
            StatusCode::NotFound
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// Тест проверяет, что по умолчанию статус ошибки — `Internal`.
    #[test]
    fn test_default_status_code_is_internal() {
        let e = DefaultError("oops");
        assert_eq!(
            e.status_code(),
            StatusCode::Internal,
            "DefaultError должен иметь статус Internal"
        );
    }

    /// Тест проверяет, что для внутренних ошибок `client_message` возвращает
    /// safe-строку.
    #[test]
    fn test_client_message_internal() {
        let e = DefaultError("sensitive");
        assert_eq!(
            e.client_message(),
            "Internal server error".to_string(),
            "client_message для внутренних ошибок должен быть безопасным"
        );
    }

    /// Тест проверяет, что для не-internal кодов `client_message` возвращает
    /// `Display`.
    #[test]
    fn test_client_message_non_internal() {
        let e = NotFoundError("nope");
        assert_eq!(
            e.client_message(),
            e.to_string(),
            "client_message для клиентских ошибок должен возвращать Display"
        );
    }

    /// Тест проверяет, что `as_any` позволяет выполнить downcast к конкретному
    /// типу.
    #[test]
    fn test_as_any_downcast() {
        let e = NotFoundError("x");
        let any = e.as_any();
        let down = any.downcast_ref::<NotFoundError>();
        assert!(
            down.is_some(),
            "as_any должен позволять downcast к исходному типу"
        );
        assert_eq!(down.unwrap().0, "x");
    }

    /// Тест проверяет, что `log_message` соответствует `Debug`-формату.
    #[test]
    fn test_log_message_matches_debug() {
        let e = NotFoundError("dbg");
        let expected = format!("{:?}", e);
        assert_eq!(
            e.log_message(),
            expected,
            "log_message должен содержать Debug-представление ошибки"
        );
    }

    /// Тест проверяет, что `metrics_tags` содержит `error_type` и
    /// `status_code`.
    #[test]
    fn test_metrics_tags_contains_expected_pairs() {
        let e = NotFoundError("t");
        let tags = e.metrics_tags();
        let mut found_type = false;
        let mut found_code = false;
        for (k, v) in tags.iter() {
            if *k == "error_type" {
                found_type = true;
                // Проверяем, что имя типа присутствует и непустое.
                assert!(
                    !v.is_empty(),
                    "Значение 'error_type' не должно быть пустым, got: '{v}'"
                );
            }
            if *k == "status_code" {
                found_code = true;
                assert_eq!(
                    v,
                    &StatusCode::NotFound.to_string(),
                    "status_code должен соответствовать коду ошибки"
                );
            }
        }
        assert!(
            found_type,
            "metrics_tags должен содержать ключ 'error_type'"
        );
        assert!(
            found_code,
            "metrics_tags должен содержать ключ 'status_code'"
        );
    }

    /// Тест проверяет, что `type_name` возвращает короткое имя типа (без
    /// модулей).
    #[test]
    fn test_type_name_returns_short_struct_name() {
        let e = NotFoundError("n");
        let tn = e.type_name();
        // Ожидаем, что имя содержит 'NotFoundError' как суффикс
        assert!(
            tn.ends_with("NotFoundError"),
            "type_name должен оканчиваться на 'NotFoundError', got: {tn}"
        );
    }
}
