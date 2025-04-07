use std::collections::HashMap;

use crate::database::{ArcBytes, Value};

/// Представляет один фрейм (единицу данных) протокола ZSP (Zumic Serialization Protocol).
///
/// Используется для сериализации/десериализации данных между клиентом и сервером.
#[derive(Debug, Clone, PartialEq)]
pub enum ZSPFrame {
    SimpleString(String),
    Error(String),
    Integer(i64),
    Float(f64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<ZSPFrame>>),
    Dictionary(Option<HashMap<String, ZSPFrame>>),
}

impl From<Value> for ZSPFrame {
    fn from(value: Value) -> Self {
        match value {
            Value::Str(s) => ZSPFrame::SimpleString(String::from_utf8_lossy(&s).to_string()),
            Value::Int(i) => ZSPFrame::Integer(i),
            Value::Float(f) => ZSPFrame::Float(f),
            Value::Bool(b) => ZSPFrame::SimpleString(b.to_string()),
            Value::List(list) => ZSPFrame::Array(Some(
                list.iter()
                    .map(|item| ZSPFrame::SimpleString(item.to_string()))
                    .collect(),
            )),
            Value::Set(set) => ZSPFrame::Array(Some(
                set.iter()
                    .map(|s| ZSPFrame::SimpleString(s.clone()))
                    .collect(),
            )),
            Value::Hash(hash) => ZSPFrame::Dictionary(Some(
                hash.into_iter()
                    .map(|(k, v)| (k.to_string(), ZSPFrame::from(v)))
                    .collect(),
            )),
            Value::ZSet { dict, sorted } => {
                let dict_frame = dict
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            String::from_utf8_lossy(&k).to_string(),
                            ZSPFrame::BulkString(Some(v.to_string().into_bytes())),
                        )
                    })
                    .collect::<HashMap<_, _>>();

                let sorted_frame = sorted
                    .into_iter()
                    .flat_map(|(_, set)| {
                        // Убираем использование key
                        set.into_iter().map(|item| {
                            ZSPFrame::SimpleString(String::from_utf8_lossy(&item).to_string())
                        })
                    })
                    .collect::<Vec<_>>(); // Теперь получаем Vec<ZSPFrame> вместо Vec<Vec<ZSPFrame>>

                ZSPFrame::Array(Some(vec![
                    ZSPFrame::Dictionary(Some(dict_frame)),
                    ZSPFrame::Array(Some(sorted_frame)),
                ]))
            }
            // Для HyperLogLog и других типов необходимо обработать аналогично
            _ => ZSPFrame::SimpleString("Unsupported type".to_string()),
        }
    }
}

impl From<ArcBytes> for ZSPFrame {
    fn from(value: ArcBytes) -> Self {
        ZSPFrame::BulkString(Some(value.to_vec()))
    }
}
