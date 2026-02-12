use std::time::{SystemTime, UNIX_EPOCH};

use crate::{CommandExecute, Sds, StorageEngine, StoreError, Value};

/// Команда PING — проверка соединения с сервером.
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
