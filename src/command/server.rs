//! Служебные команды сервера Zumic.
//!
//! Реализует команды PING, ECHO, INFO, DBSIZE, TIME, SELECT для управления
//! сервером и получения информации о его состоянии.
//! Каждая команда реализует трейт [`CommandExecute`].

use std::time::{SystemTime, UNIX_EPOCH};

use crate::{CommandExecute, Sds, StorageEngine, StoreError, Value};

/// Команда PING — проверка соединения с сервером.
///
/// Формат: `PING [message]`
///
/// # Поля
/// * `message` — необязательное сообщение для возврата.
///
/// # Возвращает
/// * Если `message` указано — возвращает это сообщение.
/// * Если `message` не указано — возвращает "PONG".
#[derive(Debug)]
pub struct PingCommand {
    pub message: Option<String>,
}

impl CommandExecute for PingCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        match &self.message {
            Some(msg) => Ok(Value::Str(Sds::from_str(msg))),
            None => Ok(Value::Str(Sds::from_str("PONG"))),
        }
    }

    fn command_name(&self) -> &'static str {
        "PING"
    }
}

/// Команда ECHO — возвращает переданное сообщение.
///
/// Формат: `ECHO message`
///
/// # Поля
/// * `message` — сообщение для возврата.
///
/// # Возвращает
/// Переданное сообщение.
#[derive(Debug)]
pub struct EchoCommand {
    pub message: String,
}

impl CommandExecute for EchoCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        Ok(Value::Str(Sds::from_str(&self.message)))
    }

    fn command_name(&self) -> &'static str {
        "ECHO"
    }
}

/// Команда DBSIZE — возвращает количество ключей в текущей базе данных.
///
/// Формат: `DBSIZE`
///
/// # Возвращает
/// Количество ключей в БД.
#[derive(Debug)]
pub struct DbSizeCommand;

impl CommandExecute for DbSizeCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let size = store.dbsize()?;
        Ok(Value::Int(size as i64))
    }

    fn command_name(&self) -> &'static str {
        "DBSIZE"
    }
}

/// Команда INFO — возвращает информацию о сервере.
///
/// Формат: `INFO [section]`
///
/// # Поля
/// * `section` — необязательная секция (Server, Memory, Stats, и т.д.).
///
/// # Возвращает
/// Строку с информацией о сервере в формате key:value.
#[derive(Debug)]
pub struct InfoCommand {
    pub section: Option<String>,
}

impl CommandExecute for InfoCommand {
    fn execute(
        &self,
        store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        let mut info = String::new();
        match self.section.as_deref() {
            Some("server") | None => {
                info.push_str("# Server\r\n");
                info.push_str("zumic_version:0.4.0\r\n");
                info.push_str("zumic_mode:standalone\r\n");
                info.push_str("os:Linux\r\n");
                info.push_str("arch_bits:64\r\n");
            }
            Some("memory") => {
                info.push_str("# Memory\r\n");
                info.push_str("used_memory:0\r\n");
                info.push_str("used_memory_human:0B\r\n");
            }
            Some("stats") => {
                info.push_str("# Stats\r\n");
                let dbsize = store.dbsize().unwrap_or(0);
                info.push_str(&format!("total_keys:{dbsize}\r\n"));
                info.push_str("total_commands_processed:0\r\n");
            }
            Some(section) => {
                return Err(StoreError::InvalidArgument(format!(
                    "Unknown section: {section}"
                )));
            }
        }
        Ok(Value::Str(Sds::from_str(&info)))
    }

    fn command_name(&self) -> &'static str {
        "INFO"
    }
}

/// Команда TIME — возвращает текущее время сервера.
///
/// Формат: `TIME`
///
/// # Возвращает
/// Массив из двух элементов:
/// * Unix timestamp в секундах
/// * Микросекунды
#[derive(Debug)]
pub struct TimeCommand;

impl CommandExecute for TimeCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| StoreError::InvalidArgument(format!("Time error: {e}")))?;

        let seconds = now.as_secs();
        let micros = now.subsec_micros();

        // Возвращаем как массив двух чисел
        Ok(Value::Array(vec![
            Value::Int(seconds as i64),
            Value::Int(micros as i64),
        ]))
    }

    fn command_name(&self) -> &'static str {
        "TIME"
    }
}

/// Команда SELECT — выбирает базу данных по индексу.
///
/// Формат: `SELECT index`
///
/// # Поля
/// * `db` — индекс базы данных (0-15).
///
/// # Возвращает
/// "OK" при успешном переключении.
///
/// # Примечание
/// В текущей версии Zumic может не поддерживать множественные БД,
/// поэтому команда может всегда возвращать ошибку или принимать только 0.
#[derive(Debug)]
pub struct SelectCommand {
    pub db: usize,
}

impl CommandExecute for SelectCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        // В Zumic пока поддерживается только одна БД (индекс 0)
        if self.db == 0 {
            Ok(Value::Str(Sds::from_str("OK")))
        } else {
            Err(StoreError::InvalidArgument(format!(
                "DB index is out of range. Zumic supports only DB 0, got {}",
                self.db
            )))
        }
    }

    fn command_name(&self) -> &'static str {
        "SELECT"
    }
}

/// Команда SAVE — синхронное сохранение БД на диск.
///
/// Формат: `SAVE`
///
/// # Возвращает
/// "OK" после успешного сохранения.
///
/// # Примечание
/// Блокирует сервер до завершения сохранения.
#[derive(Debug)]
pub struct SaveCommand;

impl CommandExecute for SaveCommand {
    fn execute(
        &self,
        store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        store.save()?;
        Ok(Value::Str(Sds::from_str("OK")))
    }

    fn command_name(&self) -> &'static str {
        "SAVE"
    }
}

/// Команда BGSAVE — асинхронное (фоновое) сохранение БД на диск.
///
/// Формат: `BGSAVE`
///
/// # Возвращает
/// "Background saving started" если сохранение запущено.
///
/// # Примечание
/// В текущей реализации может выполняться синхронно,
/// в зависимости от реализации StorageEngine.
#[derive(Debug)]
pub struct BgSaveCommand;

impl CommandExecute for BgSaveCommand {
    fn execute(
        &self,
        store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        // TODO: Реализовать асинхронное сохранение
        store.save()?;
        Ok(Value::Str(Sds::from_str("Background saving started")))
    }

    fn command_name(&self) -> &'static str {
        "BGSAVE"
    }
}

/// Команда SHUTDOWN — корректное завершение работы сервера.
///
/// Формат: `SHUTDOWN [SAVE|NOSAVE]`
///
/// # Поля
/// * `save` — если `true`, сохраняет БД перед выключением.
///
/// # Возвращает
/// Обычно не возвращает ответ, т.к. сервер завершает работу.
///
/// # Примечание
/// Эта команда требует специальной обработки на уровне сервера,
/// т.к. должна завершить все соединения и остановить event loop.
#[derive(Debug)]
pub struct ShutdownCommand {
    pub save: bool,
}

impl CommandExecute for ShutdownCommand {
    fn execute(
        &self,
        store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        if self.save {
            store.save()?;
        }
        // В реальности здесь должен быть сигнал серверу о завершении
        // Возвращаем специальное значение для обработки на уровне сервера
        Err(StoreError::ServerShutdown)
    }

    fn command_name(&self) -> &'static str {
        "SHUTDOWN"
    }
}
