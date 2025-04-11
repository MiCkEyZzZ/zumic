use super::command::Command as RawCommand;
use crate::{
    command::Command as ExeCommand,
    database::{ArcBytes, Value},
    network::zsp::frame::types::ZSPFrame,
};

/// RawCommand → ExeCommand
trait IntoExecutable {
    fn into_executable(self) -> Result<ExeCommand, String>;
}

impl IntoExecutable for RawCommand {
    fn into_executable(self) -> Result<ExeCommand, String> {
        match self {
            RawCommand::Set { key, value } => {
                Ok(ExeCommand::Set(crate::command::SetCommand { key, value }))
            }
            RawCommand::Get { key } => Ok(ExeCommand::Get(crate::command::GetCommand { key })),
            RawCommand::Del { key } => Ok(ExeCommand::Del(crate::command::DelCommand { key })),
            RawCommand::Ping => Err("Ping is not implemented yet in executable layer".to_string()),
            RawCommand::Echo(_) => {
                Err("ECHO is not implemented yet in executable layer".to_string())
            }
        }
    }
}

/// Основная точка входа: парсинг фрейма и преобразование в исполняемую команду.
pub fn parse_command(frame: ZSPFrame) -> Result<ExeCommand, String> {
    let raw_cmd = parse_raw_command(frame)?;
    raw_cmd.into_executable()
}

/// Промежуточный шаг: ZSPFrame → RawCommand
fn parse_raw_command(frame: ZSPFrame) -> Result<RawCommand, String> {
    match frame {
        ZSPFrame::Array(Some(items)) if !items.is_empty() => {
            if let ZSPFrame::SimpleString(cmd) = &items[0] {
                return parse_from_str_command(cmd, &items);
            }

            if let ZSPFrame::BulkString(Some(bytes)) = &items[0] {
                let cmd_str = String::from_utf8(bytes.clone())
                    .map_err(|_| "Invalid UTF-8 in command".to_string())?;
                return parse_from_str_command(&cmd_str, &items);
            }

            Err("Command must be a string".to_string())
        }
        _ => Err("Expected array for command".to_string()),
    }
}

/// Парсинг строки команды и аргументов из массива ZSPFrame → RawCommand
fn parse_from_str_command(cmd: &str, items: &[ZSPFrame]) -> Result<RawCommand, String> {
    match cmd.to_ascii_lowercase().as_str() {
        "ping" => Ok(RawCommand::Ping),
        "set" => {
            if items.len() != 3 {
                return Err("SET requires 2 arguments".to_string());
            }

            let key = parse_key(&items[1], "SET")?;
            let value = parse_value(&items[2], "SET")?;
            Ok(RawCommand::Set { key, value })
        }
        "get" => {
            if items.len() != 2 {
                return Err("GET requires 1 argument".to_string());
            }

            let key = parse_key(&items[1], "GET")?;
            Ok(RawCommand::Get { key })
        }
        "del" => {
            if items.len() != 2 {
                return Err("DEL requires 1 argument".to_string());
            }

            let key = parse_key(&items[1], "DEL")?;
            Ok(RawCommand::Del { key })
        }
        _ => Err("Unknown command".to_string()),
    }
}

fn parse_key(frame: &ZSPFrame, cmd: &str) -> Result<String, String> {
    match frame {
        ZSPFrame::SimpleString(s) => Ok(s.clone()),
        ZSPFrame::BulkString(Some(bytes)) => {
            String::from_utf8(bytes.clone()).map_err(|_| format!("{cmd}: key must be valid UTF-8"))
        }
        _ => Err(format!("{cmd}: invalid key")),
    }
}

fn parse_value(frame: &ZSPFrame, cmd: &str) -> Result<Value, String> {
    match frame {
        ZSPFrame::SimpleString(s) => Ok(Value::Str(ArcBytes::from_str(s))),
        ZSPFrame::BulkString(Some(bytes)) => Ok(Value::Str(ArcBytes::from(bytes.clone()))),
        ZSPFrame::Integer(n) => Ok(Value::Int(*n)),
        _ => Err(format!("{cmd}: unsupported value type")),
    }
}
