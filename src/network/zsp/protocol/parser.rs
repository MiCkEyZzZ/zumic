use super::command::Command as ZSPCommand;
use crate::{
    command::Command as StoreCommand,
    database::{Sds, Value},
    error::ParseError,
    network::zsp::frame::zsp_types::ZSPFrame,
};

/// RawCommand → ExeCommand
trait IntoExecutable {
    fn into_executable(self) -> Result<StoreCommand, ParseError>;
}

impl IntoExecutable for ZSPCommand {
    fn into_executable(self) -> Result<StoreCommand, ParseError> {
        match self {
            ZSPCommand::Set { key, value } => {
                Ok(StoreCommand::Set(crate::command::SetCommand { key, value }))
            }
            ZSPCommand::Get { key } => Ok(StoreCommand::Get(crate::command::GetCommand { key })),
            ZSPCommand::Del { key } => Ok(StoreCommand::Del(crate::command::DelCommand { key })),
            ZSPCommand::MSet { entries } => {
                Ok(StoreCommand::MSet(crate::command::MSetCommand { entries }))
            }
            ZSPCommand::MGet { keys } => {
                Ok(StoreCommand::MGet(crate::command::MGetCommand { keys }))
            }
            ZSPCommand::SetNX { key, value } => {
                Ok(StoreCommand::Setnx(crate::command::SetNxCommand {
                    key,
                    value,
                }))
            }
            ZSPCommand::Rename { from, to } => {
                Ok(StoreCommand::Rename(crate::command::RenameCommand {
                    from,
                    to,
                }))
            }
            ZSPCommand::RenameNX { from, to } => {
                Ok(StoreCommand::Renamenx(crate::command::RenameNxCommand {
                    from,
                    to,
                }))
            }
            ZSPCommand::Auth { user, pass } => {
                Ok(StoreCommand::Auth(crate::command::AuthCommand {
                    user: user.unwrap(),
                    pass,
                }))
            }

            ZSPCommand::Ping => Err(ParseError::UnknownCommand),
            ZSPCommand::Echo(_) => Err(ParseError::UnknownCommand),
        }
    }
}

/// Main entry point: parsing a frame and converting it into an executable command.
pub fn parse_command(frame: ZSPFrame) -> Result<StoreCommand, ParseError> {
    let raw_cmd = parse_raw_command(frame)?;
    raw_cmd.into_executable()
}

/// Intermediate step: ZSPFrame → RawCommand
fn parse_raw_command(frame: ZSPFrame) -> Result<ZSPCommand, ParseError> {
    match frame {
        ZSPFrame::Array(items) if !items.is_empty() => {
            if let ZSPFrame::InlineString(cmd) = &items[0] {
                return parse_from_str_command(cmd, &items);
            }

            if let ZSPFrame::BinaryString(Some(bytes)) = &items[0] {
                let cmd_str =
                    String::from_utf8(bytes.clone()).map_err(|_| ParseError::InvalidUtf8)?;
                return parse_from_str_command(&cmd_str, &items);
            }

            Err(ParseError::CommandMustBeString)
        }
        _ => Err(ParseError::ExpectedArray),
    }
}

/// Parsing command string and arguments from ZSPFrame array → RawCommand
fn parse_from_str_command(cmd: &str, items: &[ZSPFrame]) -> Result<ZSPCommand, ParseError> {
    match cmd.to_ascii_lowercase().as_str() {
        "ping" => Ok(ZSPCommand::Ping),
        "set" => {
            if items.len() != 3 {
                return Err(ParseError::WrongArgCount("SET", 2));
            }

            let key = parse_key(&items[1], "SET")?;
            let value = parse_value(&items[2], "SET")?;
            Ok(ZSPCommand::Set { key, value })
        }
        "get" => {
            if items.len() != 2 {
                return Err(ParseError::WrongArgCount("GET", 1));
            }

            let key = parse_key(&items[1], "GET")?;
            Ok(ZSPCommand::Get { key })
        }
        "del" => {
            if items.len() != 2 {
                return Err(ParseError::WrongArgCount("DEL", 1));
            }

            let key = parse_key(&items[1], "DEL")?;
            Ok(ZSPCommand::Del { key })
        }
        "mset" => {
            if items.len() < 3 || items.len() % 2 == 0 {
                return Err(ParseError::MSetWrongArgCount);
            }

            let mut parsed = Vec::new();
            for pair in items[1..].chunks(2) {
                let key = parse_key(&pair[0], "MSET")?;
                let value = parse_value(&pair[1], "MSET")?;
                parsed.push((key, value));
            }
            Ok(ZSPCommand::MSet { entries: parsed })
        }
        "mget" => {
            if items.len() < 2 {
                return Err(ParseError::WrongArgCount("MGET", 1));
            }

            let keys = items[1..]
                .iter()
                .map(|f| parse_key(f, "MGET"))
                .collect::<Result<_, _>>()?;
            Ok(ZSPCommand::MGet { keys })
        }
        "setnx" => {
            if items.len() != 3 {
                return Err(ParseError::WrongArgCount("SETNX", 2));
            }

            let key = parse_key(&items[1], "SETNX")?;
            let value = parse_value(&items[2], "SETNX")?;
            Ok(ZSPCommand::SetNX { key, value })
        }
        "rename" => {
            if items.len() != 3 {
                return Err(ParseError::WrongArgCount("RENAME", 2));
            }

            let from = parse_key(&items[1], "RENAME")?;
            let to = parse_key(&items[2], "RENAME")?;
            Ok(ZSPCommand::Rename { from, to })
        }
        "renamenx" => {
            if items.len() != 3 {
                return Err(ParseError::WrongArgCount("RENAMENX", 2));
            }

            let from = parse_key(&items[1], "RENAMENX")?;
            let to = parse_key(&items[2], "RENAMENX")?;
            Ok(ZSPCommand::RenameNX { from, to })
        }
        "auth" => {
            // AUTH <password> или AUTH <user> <password>
            match items.len() {
                2 => {
                    let pass = parse_key(&items[1], "AUTH")?;
                    Ok(ZSPCommand::Auth { user: None, pass })
                }
                3 => {
                    let user = parse_key(&items[1], "AUTH")?;
                    let pass = parse_key(&items[2], "AUTH")?;
                    Ok(ZSPCommand::Auth {
                        user: Some(user),
                        pass,
                    })
                }
                _ => return Err(ParseError::WrongArgCount("AUTH", 1)),
            }
        }
        _ => Err(ParseError::UnknownCommand),
    }
}

fn parse_key(frame: &ZSPFrame, cmd: &'static str) -> Result<String, ParseError> {
    match frame {
        ZSPFrame::InlineString(s) => Ok(s.to_string()),
        ZSPFrame::BinaryString(Some(bytes)) => {
            String::from_utf8(bytes.clone()).map_err(|_| ParseError::KeyNotUtf8(cmd))
        }
        _ => Err(ParseError::InvalidKey(cmd)),
    }
}

fn parse_value(frame: &ZSPFrame, cmd: &'static str) -> Result<Value, ParseError> {
    match frame {
        ZSPFrame::InlineString(s) => Ok(Value::Str(Sds::from_str(s))),
        ZSPFrame::BinaryString(Some(bytes)) => Ok(Value::Str(Sds::from_vec(bytes.clone()))),
        ZSPFrame::Integer(n) => Ok(Value::Int(*n)),
        _ => Err(ParseError::InvalidValueType(cmd)),
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    /// Проверяет корректный парсинг команды SET с двумя строковыми аргументами (ключ и значение)
    #[test]
    fn test_parse_set_command() {
        let frame = ZSPFrame::Array(vec![
            ZSPFrame::InlineString(Cow::Borrowed("SET")),
            ZSPFrame::InlineString(Cow::Borrowed("anton")),
            ZSPFrame::InlineString(Cow::Borrowed("hisvalue")),
        ]);

        let set_cmd = parse_command(frame).unwrap();

        match set_cmd {
            StoreCommand::Set(set) => {
                assert_eq!(set.key, "anton");
                assert_eq!(set.value, Value::Str(Sds::from_str("hisvalue")));
            }
            _ => panic!("Expected SetCommand"),
        }
    }

    /// Проверяет парсинг команды GET с аргументом в виде BinaryString
    #[test]
    fn test_parse_get_command_with_bulk_key() {
        let frame = ZSPFrame::Array(vec![
            ZSPFrame::BinaryString(Some(b"GET".to_vec())),
            ZSPFrame::BinaryString(Some(b"anton".to_vec())),
        ]);

        let get_cmd = parse_command(frame).unwrap();

        match get_cmd {
            StoreCommand::Get(get) => {
                assert_eq!(get.key, "anton")
            }
            _ => panic!("Expected GetCommand"),
        }
    }

    /// Проверяет парсинг команды DEL с ключом в виде InlineString
    #[test]
    fn test_parse_del_command_with_simple_key() {
        let frame = ZSPFrame::Array(vec![
            ZSPFrame::InlineString(Cow::Borrowed("DEL")),
            ZSPFrame::InlineString(Cow::Borrowed("anton")),
        ]);

        let del_cmd = parse_command(frame).unwrap();

        match del_cmd {
            StoreCommand::Del(del) => {
                assert_eq!(del.key, "anton");
            }
            _ => panic!("Expected DelCommand"),
        }
    }

    /// Проверяет парсинг SET с числовым значением
    #[test]
    fn test_parse_set_command_with_int_value() {
        let frame = ZSPFrame::Array(vec![
            ZSPFrame::InlineString(Cow::Borrowed("SET")),
            ZSPFrame::InlineString(Cow::Borrowed("count")),
            ZSPFrame::Integer(42),
        ]);

        let set_cmd = parse_command(frame).unwrap();

        match set_cmd {
            StoreCommand::Set(set) => {
                assert_eq!(set.key, "count");
                assert_eq!(set.value, Value::Int(42));
            }
            _ => panic!("Expected SetCommand with Int value"),
        }
    }

    /// Проверяет поведение при неизвестной команде
    #[test]
    fn test_unknown_command() {
        let frame = ZSPFrame::Array(vec![ZSPFrame::InlineString(Cow::Borrowed("KIN"))]);

        let err = parse_command(frame).unwrap_err();
        assert_eq!(err.to_string(), "Unknown command");
    }

    /// Проверяет ошибку при слишком большом числе аргументов в GET
    #[test]
    fn test_get_command_with_too_many_args() {
        let frame = ZSPFrame::Array(vec![
            ZSPFrame::InlineString(Cow::Borrowed("GET")),
            ZSPFrame::InlineString(Cow::Borrowed("anton")),
            ZSPFrame::InlineString(Cow::Borrowed("hisvalue")),
        ]);

        let err = parse_command(frame).unwrap_err();
        assert_eq!(err.to_string(), "GET requires 1 argument(s)");
    }

    /// Проверяет ошибку, если ключ передан некорректного типа (Integer)
    #[test]
    fn test_set_command_with_invalid_key_type() {
        let frame = ZSPFrame::Array(vec![
            ZSPFrame::InlineString(Cow::Borrowed("SET")),
            ZSPFrame::Integer(123),
            ZSPFrame::InlineString(Cow::Borrowed("value")),
        ]);

        let err = parse_command(frame).unwrap_err();
        assert_eq!(err.to_string(), "SET: invalid key");
    }

    /// Проверяет ошибку, если команда не представлена массивом
    #[test]
    fn test_command_not_array() {
        let frame = ZSPFrame::InlineString(Cow::Borrowed("SET"));
        let err = parse_command(frame).unwrap_err();
        assert_eq!(err.to_string(), "Expected array for command");
    }

    /// Проверяет ошибку при пустом массиве команды
    #[test]
    fn test_command_array_empty() {
        let frame = ZSPFrame::Array(vec![]);
        let err = parse_command(frame).unwrap_err();
        assert_eq!(err.to_string(), "Expected array for command");
    }
}
