use super::command::Command as ZSPCommand;
use crate::{
    command::Command as StoreCommand,
    database::{ArcBytes, Value},
    network::zsp::frame::zsp_types::ZSPFrame,
};

/// RawCommand → ExeCommand
trait IntoExecutable {
    fn into_executable(self) -> Result<StoreCommand, String>;
}

impl IntoExecutable for ZSPCommand {
    fn into_executable(self) -> Result<StoreCommand, String> {
        match self {
            ZSPCommand::Set { key, value } => {
                Ok(StoreCommand::Set(crate::command::SetCommand { key, value }))
            }
            ZSPCommand::Get { key } => Ok(StoreCommand::Get(crate::command::GetCommand { key })),
            ZSPCommand::Del { key } => Ok(StoreCommand::Del(crate::command::DelCommand { key })),
            ZSPCommand::Ping => Err("Ping is not implemented yet in executable layer".to_string()),
            ZSPCommand::Echo(_) => {
                Err("ECHO is not implemented yet in executable layer".to_string())
            }
        }
    }
}

/// Основная точка входа: парсинг фрейма и преобразование в исполняемую команду.
pub fn parse_command(frame: ZSPFrame) -> Result<StoreCommand, String> {
    let raw_cmd = parse_raw_command(frame)?;
    raw_cmd.into_executable()
}

/// Промежуточный шаг: ZSPFrame → RawCommand
fn parse_raw_command(frame: ZSPFrame) -> Result<ZSPCommand, String> {
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
fn parse_from_str_command(cmd: &str, items: &[ZSPFrame]) -> Result<ZSPCommand, String> {
    match cmd.to_ascii_lowercase().as_str() {
        "ping" => Ok(ZSPCommand::Ping),
        "set" => {
            if items.len() != 3 {
                return Err("SET requires 2 arguments".to_string());
            }

            let key = parse_key(&items[1], "SET")?;
            let value = parse_value(&items[2], "SET")?;
            Ok(ZSPCommand::Set { key, value })
        }
        "get" => {
            if items.len() != 2 {
                return Err("GET requires 1 argument".to_string());
            }

            let key = parse_key(&items[1], "GET")?;
            Ok(ZSPCommand::Get { key })
        }
        "del" => {
            if items.len() != 2 {
                return Err("DEL requires 1 argument".to_string());
            }

            let key = parse_key(&items[1], "DEL")?;
            Ok(ZSPCommand::Del { key })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_set_command() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("SET".to_string()),
            ZSPFrame::SimpleString("anton".to_string()),
            ZSPFrame::SimpleString("hisvalue".to_string()),
        ]));

        let set_cmd = parse_command(frame).unwrap();

        match set_cmd {
            StoreCommand::Set(set) => {
                assert_eq!(set.key, "anton");
                assert_eq!(set.value, Value::Str(ArcBytes::from_str("hisvalue")));
            }
            _ => panic!("Expected SetCommand"),
        }
    }

    #[test]
    fn test_parse_get_command_with_bulk_key() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::BulkString(Some(b"GET".to_vec())),
            ZSPFrame::BulkString(Some(b"anton".to_vec())),
        ]));

        let get_cmd = parse_command(frame).unwrap();

        match get_cmd {
            StoreCommand::Get(get) => {
                assert_eq!(get.key, "anton")
            }
            _ => panic!("Expected GetCommand"),
        }
    }

    #[test]
    fn test_parse_del_command_with_simple_key() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("DEL".to_string()),
            ZSPFrame::SimpleString("anton".to_string()),
        ]));

        let del_cmd = parse_command(frame).unwrap();

        match del_cmd {
            StoreCommand::Del(del) => {
                assert_eq!(del.key, "anton");
            }
            _ => panic!("Expected DelCommand"),
        }
    }

    #[test]
    fn test_parse_set_command_with_int_value() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("SET".to_string()),
            ZSPFrame::SimpleString("count".to_string()),
            ZSPFrame::Integer(42),
        ]));

        let set_cmd = parse_command(frame).unwrap();

        match set_cmd {
            StoreCommand::Set(set) => {
                assert_eq!(set.key, "count");
                assert_eq!(set.value, Value::Int(42));
            }
            _ => panic!("Expected SetCommand with Int value"),
        }
    }

    #[test]
    fn test_unknown_command() {
        let frame = ZSPFrame::Array(Some(vec![ZSPFrame::SimpleString("KIN".to_string())]));

        let err = parse_command(frame).unwrap_err();
        assert_eq!(err, "Unknown command");
    }

    #[test]
    fn test_get_command_with_too_many_args() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("GET".to_string()),
            ZSPFrame::SimpleString("anton".to_string()),
            ZSPFrame::SimpleString("hisvalue".to_string()),
        ]));

        let err = parse_command(frame).unwrap_err();
        assert_eq!(err, "GET requires 1 argument");
    }

    #[test]
    fn test_set_command_with_invalid_key_type() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("SET".to_string()),
            ZSPFrame::Integer(123), // invalid key
            ZSPFrame::SimpleString("value".to_string()),
        ]));

        let err = parse_command(frame).unwrap_err();
        assert_eq!(err, "SET: invalid key");
    }

    #[test]
    fn test_command_not_array() {
        let frame = ZSPFrame::SimpleString("SET".to_string());
        let err = parse_command(frame).unwrap_err();
        assert_eq!(err, "Expected array for command");
    }

    #[test]
    fn test_command_array_empty() {
        let frame = ZSPFrame::Array(Some(vec![]));
        let err = parse_command(frame).unwrap_err();
        assert_eq!(err, "Expected array for command");
    }
}
