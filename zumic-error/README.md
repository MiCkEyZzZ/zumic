# zumic-error

Централизовання система обработки ошибок для Zumic.

## Особенности

- **Типизированный статус-коды** - чёткая категоризация ошибок
- **Location tracking** - автоматическая трассировка места возникновения
- **Контекстные цепочки** - добавление контекста по мере распространения
- **Метрики для observability** - готовые теги для Prometheus/DataDog
- **Безопасность** - разделение сообщений для клиентов и логов
- **Zero-cost abstractions** - минимальные накладные расходы

## Использование

### Базовый пример

```rust
use zumic-error::{ZumicResult, types::auth::AuthError, bail, ensure};

fn authenticate(username: &str, password: &str) -> ZumicResult {
  ensure!(
    !username.is_empty(),
    StatusCode::InvalidArgs,
    "Username cannot be empty"
  );

  let user = find_user(username)
    .ok_or_else(|| AuthError::UserNotFound {
      username: username.to_string(),
    })?;

  verify_password(password, &user.password_hash)
    .context("Password verification failed")?;

  Ok(create_session(user))
}
```

### Добавления контекста

```rust
use zumic_error::ResultExt;

fn process_request(req: Reuest) -> ZumicResult {
  parser_request(&req)
    .context("Failed to parse request")?;

  validate_request(&req)
    .with_context(|| format!("Validation failed for request ID: {}", req.id))?;

  execute_command(&req)
    .context("Command execution failed")?;

  Ok(build_response())
}
```

### Создание собственных ошибок

```rust
use zumic_error::{ErrorExt, StatusCode};
use std::any::Any;

#[derive(Debug, Clone)]
pub enum MyError {
  Custom { message: String },
}

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Custom { message } => write!(f, "Custom error: {message}"),
        }
    }
}

impl std::error::Error for MyError {}

impl ErrorExt for MyError {
  fn status_code(&self) -> StatusCode {
    StatusCode::Internal
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}
```

### HTTP API интеграция (с feature "serde")

```rust
use zumic_error::StackError;
use axum::{response::IntoResponse, http::StatusCode as HttpStatus};

impl IntoResponse for StackError {
    fn into_response(self) -> axum::response::Response {
        let status = HttpStatus::from_u16(self.status_code().http_status())
            .unwrap_or(HttpStatus::INTERNAL_SERVER_ERROR);

        let body = serde_json::to_string(&self.to_response())
            .unwrap_or_else(|_| r#"{"error":"Internal server error"}"#.to_string());

        (status, body).into_response()
    }
}
```

## Поддерживаемые функции

- `serde` - Сериализация ошибок для API ответов
- `tokio` - Интеграция с tokio::sync для pub/sub ошибок
- `globset` - Поддержка glob паттернов в pub/sub

## Статус-коды

| Диапазон | Категория  | Примеры                                            |
|----------|------------|----------------------------------------------------|
| 0xxx     | Success    | `Success`                                          |
| 1xxx     | General    | `Unknown`, `Internal`, `InvalidArgs`               |
| 2xxx     | Data       | `NotFound`, `AlreadyExists`, `TypeError`           |
| 3xxx     | Auth       | `AuthFailed`, `PermissionDenied`, `SessionExpired` |
| 4xxx     | Rate Limit | `RateLimited`, `TooManyConnections`                |
| 5xxx     | Storage    | `StorageUnavailable`, `DiskFull`, `CorruptedData`  |
| 6xxx     | Network    | `Timeout`, `ConnectionClosed`, `ProtocolError`     |
| 7xxx     | Cluster    | `ClusterDown`, `MovedSlot`, `CrossSlot`            |
| 8xxx     | Protocol   | `InvalidFrame`, `EncodingError`, `DecodingError`   |

## License

Apache-2.0
