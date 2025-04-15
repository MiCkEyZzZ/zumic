use std::collections::HashMap;

use super::command::Response;
use crate::{database::Value, network::zsp::frame::zsp_types::ZSPFrame};

/// Сериализует ответ команды в формат ZSPFrame.
pub fn serialize_response(response: Response) -> ZSPFrame {
    match response {
        Response::Ok => ZSPFrame::SimpleString("OK".into()),
        Response::Value(value) => value_to_frame(value), // Всё делается через вспомогательную функцию
        Response::Error(msg) => ZSPFrame::FrameError(msg),
        Response::NotFound => ZSPFrame::Null,
        Response::Integer(n) => ZSPFrame::Integer(n),
        Response::Float(f) => ZSPFrame::Float(f),
        Response::String(s) => ZSPFrame::SimpleString(s),
    }
}

/// Преобразует тип Value в ZSPFrame.
fn value_to_frame(value: Value) -> ZSPFrame {
    match value {
        Value::Str(s) => ZSPFrame::BulkString(Some(s.to_vec())),
        Value::Int(i) => ZSPFrame::Integer(i),
        Value::Float(f) => ZSPFrame::Float(f),
        Value::Null => ZSPFrame::Null,
        Value::List(list) => {
            let frames = list
                .iter()
                .map(|item| ZSPFrame::BulkString(Some(item.to_vec())))
                .collect();
            ZSPFrame::Array(Some(frames))
        }
        // Здесь Value::Hash содержит SmartHash, поэтому используем его итератор.
        Value::Hash(smart) => {
            let dict: HashMap<String, ZSPFrame> = smart
                .iter()
                .map(|(k, v)| {
                    let key =
                        String::from_utf8(k.to_vec()).unwrap_or_else(|_| "<invalid utf8>".into());
                    // Преобразуем значение в BulkString; можно при необходимости расширить логику.
                    let val = ZSPFrame::BulkString(Some(v.to_vec()));
                    (key, val)
                })
                .collect();
            ZSPFrame::Dictionary(Some(dict))
        }
        Value::ZSet { dict, .. } => {
            let pairs = dict
                .into_iter()
                .map(|(k, score)| {
                    let key =
                        String::from_utf8(k.to_vec()).unwrap_or_else(|_| "<invalid utf8>".into());
                    (key, score)
                })
                .collect();
            ZSPFrame::ZSet(pairs)
        }
        Value::Set(set) => {
            let frames = set
                .into_iter()
                .map(|item| ZSPFrame::SimpleString(item))
                .collect();
            ZSPFrame::Array(Some(frames))
        }
        Value::HyperLogLog(_) => ZSPFrame::SimpleString("HLL(NotImplemented)".into()),
        Value::SStream(_) => ZSPFrame::SimpleString("SStream(NotImplemented)".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::database::{skip_list::SkipList, ArcBytes, QuickList};

    use super::*;

    #[test]
    fn test_serialize_ok() {
        let frame = serialize_response(Response::Ok);
        assert_eq!(frame, ZSPFrame::SimpleString("OK".into()));
    }

    #[test]
    fn test_serialize_error() {
        let frame = serialize_response(Response::Error("fail".into()));
        assert_eq!(frame, ZSPFrame::FrameError("fail".into()));
    }

    #[test]
    fn test_serialize_str() {
        let value = Value::Str(ArcBytes::from_str("hello"));
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::BulkString(Some(b"hello".to_vec())));
    }

    #[test]
    fn test_serialize_int() {
        let value = Value::Int(123);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::Integer(123));
    }

    #[test]
    fn test_serialize_float() {
        let value = Value::Float(2.14);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::Float(2.14));
    }

    #[test]
    fn test_serialize_null() {
        let value = Value::Null;
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::Null);
    }

    #[test]
    fn test_serialize_list() {
        let mut list = QuickList::new(4);
        list.push_back(ArcBytes::from_str("a"));
        list.push_back(ArcBytes::from_str("b"));

        let value = Value::List(list);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(
            frame,
            ZSPFrame::Array(Some(vec![
                ZSPFrame::BulkString(Some(b"a".to_vec())),
                ZSPFrame::BulkString(Some(b"b".to_vec())),
            ]))
        );
    }

    #[test]
    fn test_serialize_set() {
        let mut set = HashSet::new();
        set.insert("x".to_string());
        set.insert("y".to_string());
        let value = Value::Set(set);
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZSPFrame::Array(Some(mut items)) => {
                let mut strings = items
                    .drain(..)
                    .map(|item| match item {
                        ZSPFrame::SimpleString(s) => s,
                        _ => panic!("Expected SimpleString"),
                    })
                    .collect::<Vec<_>>();
                strings.sort();
                assert_eq!(strings, vec!["x".to_string(), "y".to_string()]);
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_serialize_hash() {
        let mut sh = crate::database::smart_hash::SmartHash::new();
        sh.hset(ArcBytes::from_str("k1"), ArcBytes::from_str("v1"));
        let value = Value::Hash(sh);
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZSPFrame::Dictionary(Some(dict)) => {
                assert_eq!(
                    dict.get("k1"),
                    Some(&ZSPFrame::BulkString(Some(b"v1".to_vec())))
                );
            }
            _ => panic!("Expected Dictionary frame"),
        }
    }

    #[test]
    fn test_serialize_zset() {
        let mut dict = HashMap::new();
        let mut sorted = SkipList::new();

        let key = ArcBytes::from_str("one");
        let score = 1.0;
        dict.insert(key.clone(), score);
        sorted.insert(ordered_float::OrderedFloat(score), ArcBytes::from(key));

        let value = Value::ZSet { dict, sorted };
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZSPFrame::ZSet(pairs) => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0], ("one".to_string(), 1.0));
            }
            _ => panic!("Expected ZSet frame"),
        }
    }

    #[test]
    fn test_serialize_hll() {
        let hll = crate::database::types::HLL {
            registers: vec![0; 128],
        };
        let value = Value::HyperLogLog(hll);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::SimpleString("HLL(NotImplemented)".into()));
    }

    #[test]
    fn test_serialize_sstream() {
        let value = Value::SStream(vec![]);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(
            frame,
            ZSPFrame::SimpleString("SStream(NotImplemented)".into())
        );
    }
}
