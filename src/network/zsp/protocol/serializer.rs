use std::{borrow::Cow, collections::HashMap};

use super::Response;
use crate::{zsp::frame::zsp_types::ZSPFrame, Value};

/// Сериализует ответ команды в формат ZSPFrame.
pub fn serialize_response<'a>(response: Response) -> ZSPFrame<'a> {
    match response {
        Response::Ok => ZSPFrame::InlineString("OK".into()),
        Response::Value(value) => value_to_frame(value),
        Response::Error(msg) => ZSPFrame::FrameError(msg),
        Response::NotFound => ZSPFrame::Null,
        Response::Integer(n) => ZSPFrame::Integer(n),
        Response::Float(f) => ZSPFrame::Float(f),
        Response::String(s) => ZSPFrame::InlineString(s.into()),
    }
}

/// Преобразует тип Value в ZSPFrame.
fn value_to_frame<'a>(value: Value) -> ZSPFrame<'a> {
    match value {
        Value::Str(s) => ZSPFrame::BinaryString(Some(s.to_vec())),
        Value::Int(i) => ZSPFrame::Integer(i),
        Value::Float(f) => ZSPFrame::Float(f),
        Value::Null => ZSPFrame::Null,
        Value::List(list) => {
            let frames = list
                .iter()
                .map(|item| ZSPFrame::BinaryString(Some(item.to_vec())))
                .collect();
            ZSPFrame::Array(frames)
        }

        // Здесь Value::Hash содержит SmartHash, поэтому используем его итератор.
        Value::Hash(mut smart) => {
            let dict: HashMap<Cow<'a, str>, ZSPFrame<'a>> = smart
                .iter()
                .map(|(k, v)| {
                    // Преобразуем ключ в Cow<'a, str>
                    let key = Cow::from(
                        String::from_utf8(k.to_vec()).unwrap_or_else(|_| "<invalid utf8>".into()),
                    );

                    // Преобразуем значение в BinaryString, как раньше
                    let val = ZSPFrame::BinaryString(Some(v.to_vec()));

                    (key, val)
                })
                .collect();

            ZSPFrame::Dictionary(dict)
        }

        Value::ZSet { dict, .. } => {
            let pairs = dict
                .into_iter()
                .map(|(k_sds, &score)| {
                    let key = String::from_utf8(k_sds.to_vec())
                        .unwrap_or_else(|_| "<invalid utf8>".into());
                    (key, score) // score уже f64
                })
                .collect::<Vec<(String, f64)>>(); // теперь тип правильно совпадает
            ZSPFrame::ZSet(pairs)
        }

        Value::Set(set) => {
            let frames = set
                .into_iter()
                .map(|item| {
                    let s = String::from_utf8(item.to_vec())
                        .unwrap_or_else(|_| "<invalid utf8>".into());
                    ZSPFrame::InlineString(Cow::Owned(s))
                })
                .collect();
            ZSPFrame::Array(frames)
        }
        Value::HyperLogLog(_) => ZSPFrame::InlineString("HLL(NotImplemented)".into()),
        Value::SStream(_) => ZSPFrame::InlineString("SStream(NotImplemented)".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::{
        database::{skip_list::SkipList, QuickList, Sds},
        Dict,
    };

    use super::*;

    /// Проверяет сериализацию `Response::Ok` в `ZSPFrame::InlineString("OK")`
    #[test]
    fn test_serialize_ok() {
        let frame = serialize_response(Response::Ok);
        assert_eq!(frame, ZSPFrame::InlineString("OK".into()));
    }

    /// Проверяет сериализацию `Response::Error("fail")` в `ZSPFrame::FrameError("fail")`
    #[test]
    fn test_serialize_error() {
        let frame = serialize_response(Response::Error("fail".into()));
        assert_eq!(frame, ZSPFrame::FrameError("fail".into()));
    }

    /// Проверяет сериализацию `Value::Str` в `ZSPFrame::BinaryString`
    #[test]
    fn test_serialize_str() {
        let value = Value::Str(Sds::from_str("hello"));
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::BinaryString(Some(b"hello".to_vec())));
    }

    /// Проверяет сериализацию `Value::Int` в `ZSPFrame::Integer`
    #[test]
    fn test_serialize_int() {
        let value = Value::Int(123);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::Integer(123));
    }

    /// Проверяет сериализацию `Value::Float` в `ZSPFrame::Float`
    #[test]
    fn test_serialize_float() {
        let value = Value::Float(2.14);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::Float(2.14));
    }

    /// Проверяет сериализацию `Value::Null` в `ZSPFrame::Null`
    #[test]
    fn test_serialize_null() {
        let value = Value::Null;
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::Null);
    }

    /// Проверяет сериализацию `Value::List` (QuickList) в `ZSPFrame::Array`
    #[test]
    fn test_serialize_list() {
        let mut list = QuickList::new(4);
        list.push_back(Sds::from_str("a"));
        list.push_back(Sds::from_str("b"));

        let value = Value::List(list);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(
            frame,
            ZSPFrame::Array(vec![
                ZSPFrame::BinaryString(Some(b"a".to_vec())),
                ZSPFrame::BinaryString(Some(b"b".to_vec())),
            ])
        );
    }

    /// Проверяет сериализацию `Value::Set` (HashSet) в `ZSPFrame::Array` со строками
    #[test]
    fn test_serialize_set() {
        let mut set = HashSet::new();
        set.insert(Sds::from_str("x"));
        set.insert(Sds::from_str("y"));
        let value = Value::Set(set);
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZSPFrame::Array(mut items) => {
                let mut strings = items
                    .drain(..)
                    .map(|item| match item {
                        ZSPFrame::InlineString(s) => s,
                        _ => panic!("Expected InlineString"),
                    })
                    .collect::<Vec<_>>();
                strings.sort();
                assert_eq!(strings, vec!["x".to_string(), "y".to_string()]);
            }
            _ => panic!("Expected Array"),
        }
    }

    /// Проверяет сериализацию `Value::Hash` (SmartHash) в `ZSPFrame::Dictionary`
    #[test]
    fn test_serialize_hash() {
        let mut sh = crate::database::smart_hash::SmartHash::new();
        sh.insert(Sds::from_str("k1"), Sds::from_str("v1"));
        let value = Value::Hash(sh);
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZSPFrame::Dictionary(dict) => {
                assert_eq!(
                    dict.get("k1"),
                    Some(&ZSPFrame::BinaryString(Some(b"v1".to_vec())))
                );
            }
            _ => panic!("Expected Dictionary frame"),
        }
    }

    /// Проверяет сериализацию `Value::ZSet` (dict + SkipList) в `ZSPFrame::ZSet`
    #[test]
    fn test_serialize_zset() {
        let mut dict = Dict::new();
        let mut sorted = SkipList::new();

        let key = Sds::from_str("one");
        let score = 1.0;
        dict.insert(key.clone(), score);
        sorted.insert(ordered_float::OrderedFloat(score), key);

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

    /// Проверяет сериализацию `Value::HyperLogLog` в `ZSPFrame::InlineString("HLL(NotImplemented)")`
    #[test]
    fn test_serialize_hll() {
        let hll = crate::HLL { data: [0; 12288] };
        let value = Value::HyperLogLog(hll);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZSPFrame::InlineString("HLL(NotImplemented)".into()));
    }

    /// Проверяет сериализацию `Value::SStream` в `ZSPFrame::InlineString("SStream(NotImplemented)")`
    #[test]
    fn test_serialize_sstream() {
        let value = Value::SStream(vec![]);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(
            frame,
            ZSPFrame::InlineString("SStream(NotImplemented)".into())
        );
    }
}
