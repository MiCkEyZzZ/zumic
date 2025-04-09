use super::command::Command;
use crate::{
    database::{ArcBytes, Value},
    network::zsp::frame::types::ZSPFrame,
};

pub fn parse_command(frame: ZSPFrame) -> Result<Command, String> {
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

fn parse_from_str_command(cmd: &str, items: &[ZSPFrame]) -> Result<Command, String> {
    match cmd.to_ascii_lowercase().as_str() {
        "ping" => Ok(Command::Ping),
        "set" => {
            if items.len() != 3 {
                return Err("SET requires 2 arguments".to_string());
            }

            let key = match &items[1] {
                ZSPFrame::SimpleString(s) => s.clone(),
                ZSPFrame::BulkString(Some(bytes)) => String::from_utf8(bytes.clone())
                    .map_err(|_| "SET: key must be valid UTF-8".to_string())?,
                _ => return Err("SET: invalid key".to_string()),
            };

            let value = match &items[2] {
                ZSPFrame::SimpleString(s) => Value::Str(ArcBytes::from_str(s)),
                ZSPFrame::BulkString(Some(bytes)) => Value::Str(ArcBytes::from(bytes.clone())),
                ZSPFrame::Integer(n) => Value::Int(*n),
                _ => return Err("SET: unsupported value type".to_string()),
            };

            Ok(Command::Set { key, value })
        }
        "get" => {
            if items.len() != 2 {
                return Err("GET requires 1 argument".to_string());
            }

            let key = match &items[1] {
                ZSPFrame::SimpleString(s) => s.clone(),
                ZSPFrame::BulkString(Some(bytes)) => String::from_utf8(bytes.clone())
                    .map_err(|_| "GET: key must be valid UTF-8".to_string())?,
                _ => return Err("GET: invalid key".to_string()),
            };

            Ok(Command::Get { key })
        }
        "del" => {
            if items.len() != 2 {
                return Err("DEL requires 1 argument".to_string());
            }

            let key = match &items[1] {
                ZSPFrame::SimpleString(s) => s.clone(),
                ZSPFrame::BulkString(Some(bytes)) => String::from_utf8(bytes.clone())
                    .map_err(|_| "DEL: key must be valid UTF-8".to_string())?,
                _ => return Err("DEL: invalid key".to_string()),
            };

            Ok(Command::Del { key })
        }
        _ => Err("Unknown command".to_string()),
    }
}
