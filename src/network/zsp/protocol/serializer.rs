use std::{borrow::Cow, collections::HashMap};

use crate::{zsp::zsp_types::ZspFrame, Value};

use super::command::Response;

/// Сериализует ответ команды в формат ZspFrame.
pub fn serialize_response<'a>(response: Response) -> ZspFrame<'a> {
    match response {
        Response::Ok => ZspFrame::InlineString("OK".into()),
        Response::Value(value) => value_to_frame(value),
        Response::Error(msg) => ZspFrame::FrameError(msg),
        Response::NotFound => ZspFrame::Null,
        Response::Integer(n) => ZspFrame::Integer(n),
        Response::Float(f) => ZspFrame::Float(f),
        Response::Bool(b) => ZspFrame::Bool(b),
        Response::String(s) => ZspFrame::InlineString(s.into()),
    }
}

/// Преобразует тип Value в ZspFrame.
fn value_to_frame<'a>(value: Value) -> ZspFrame<'a> {
    match value {
        Value::Str(s) => ZspFrame::BinaryString(Some(s.to_vec())),
        Value::Int(i) => ZspFrame::Integer(i),
        Value::Float(f) => ZspFrame::Float(f),
        Value::Bool(b) => ZspFrame::Bool(b),
        Value::Null => ZspFrame::Null,
        Value::List(list) => {
            let frames = list
                .iter()
                .map(|item| ZspFrame::BinaryString(Some(item.to_vec())))
                .collect();
            ZspFrame::Array(frames)
        }

        Value::Array(arr) => {
            let frames = arr.into_iter().map(value_to_frame).collect();
            ZspFrame::Array(frames)
        }

        // Здесь Value::Hash содержит SmartHash, поэтому используем его итератор.
        Value::Hash(mut smart) => {
            let dict: HashMap<Cow<'a, str>, ZspFrame<'a>> = smart
                .iter()
                .map(|(k, v)| {
                    // Преобразуем ключ в Cow<'a, str>
                    let key = Cow::from(
                        String::from_utf8(k.to_vec()).unwrap_or_else(|_| "<invalid utf8>".into()),
                    );

                    // Преобразуем значение в BinaryString, как раньше
                    let val = ZspFrame::BinaryString(Some(v.to_vec()));

                    (key, val)
                })
                .collect();

            ZspFrame::Dictionary(dict)
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
            ZspFrame::ZSet(pairs)
        }

        Value::Set(set) => {
            let frames = set
                .into_iter()
                .map(|item| {
                    let s = String::from_utf8(item.to_vec())
                        .unwrap_or_else(|_| "<invalid utf8>".into());
                    ZspFrame::InlineString(Cow::Owned(s))
                })
                .collect();
            ZspFrame::Array(frames)
        }

        Value::Bitmap(bmp) => ZspFrame::BinaryString(Some(bmp.as_bytes().to_vec())),

        Value::HyperLogLog(_) => ZspFrame::InlineString("Hll(NotImplemented)".into()),

        Value::SStream(_) => ZspFrame::InlineString("SStream(NotImplemented)".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::{Dict, Hll, QuickList, Sds, SkipList, SmartHash};

    use super::*;

    /// Проверяет сериализацию `Response::Ok` в `ZspFrame::InlineString("OK")`
    #[test]
    fn test_serialize_ok() {
        let frame = serialize_response(Response::Ok);
        assert_eq!(frame, ZspFrame::InlineString("OK".into()));
    }

    /// Проверяет сериализацию `Response::Error("fail")` в `ZspFrame::FrameError("fail")`
    #[test]
    fn test_serialize_error() {
        let frame = serialize_response(Response::Error("fail".into()));
        assert_eq!(frame, ZspFrame::FrameError("fail".into()));
    }

    /// Проверяет сериализацию `Value::Str` в `ZspFrame::BinaryString`
    #[test]
    fn test_serialize_str() {
        let value = Value::Str(Sds::from_str("hello"));
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::BinaryString(Some(b"hello".to_vec())));
    }

    /// Проверяет сериализацию `Value::Int` в `ZspFrame::Integer`
    #[test]
    fn test_serialize_int() {
        let value = Value::Int(123);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::Integer(123));
    }

    /// Проверяет сериализацию `Value::Float` в `ZspFrame::Float`
    #[test]
    fn test_serialize_float() {
        let value = Value::Float(2.14);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::Float(2.14));
    }

    /// Проверяет сериализацию `Value::Null` в `ZspFrame::Null`
    #[test]
    fn test_serialize_null() {
        let value = Value::Null;
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::Null);
    }

    /// Проверяет сериализацию `Value::List` (QuickList) в `ZspFrame::Array`
    #[test]
    fn test_serialize_list() {
        let mut list = QuickList::new(4);
        list.push_back(Sds::from_str("a"));
        list.push_back(Sds::from_str("b"));

        let value = Value::List(list);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(
            frame,
            ZspFrame::Array(vec![
                ZspFrame::BinaryString(Some(b"a".to_vec())),
                ZspFrame::BinaryString(Some(b"b".to_vec())),
            ])
        );
    }

    /// Проверяет сериализацию `Value::Set` (HashSet) в `ZspFrame::Array` со строками
    #[test]
    fn test_serialize_set() {
        let mut set = HashSet::new();
        set.insert(Sds::from_str("x"));
        set.insert(Sds::from_str("y"));
        let value = Value::Set(set);
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZspFrame::Array(mut items) => {
                let mut strings = items
                    .drain(..)
                    .map(|item| match item {
                        ZspFrame::InlineString(s) => s,
                        _ => panic!("Expected InlineString"),
                    })
                    .collect::<Vec<_>>();
                strings.sort();
                assert_eq!(strings, vec!["x".to_string(), "y".to_string()]);
            }
            _ => panic!("Expected Array"),
        }
    }

    /// Проверяет сериализацию `Value::Hash` (SmartHash) в `ZspFrame::Dictionary`
    #[test]
    fn test_serialize_hash() {
        let mut sh = SmartHash::new();
        sh.insert(Sds::from_str("k1"), Sds::from_str("v1"));
        let value = Value::Hash(sh);
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZspFrame::Dictionary(dict) => {
                assert_eq!(
                    dict.get("k1"),
                    Some(&ZspFrame::BinaryString(Some(b"v1".to_vec())))
                );
            }
            _ => panic!("Expected Dictionary frame"),
        }
    }

    /// Проверяет сериализацию `Value::ZSet` (dict + SkipList) в `ZspFrame::ZSet`
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
            ZspFrame::ZSet(pairs) => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0], ("one".to_string(), 1.0));
            }
            _ => panic!("Expected ZSet frame"),
        }
    }

    /// Проверяет сериализацию `Value::HyperLogLog` в `ZspFrame::InlineString("HLL(NotImplemented)")`
    #[test]
    fn test_serialize_hll() {
        let hll = Hll { data: [0; 12288] };
        let value = Value::HyperLogLog(Box::new(hll));
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::InlineString("Hll(NotImplemented)".into()));
    }

    /// Проверяет сериализацию `Value::SStream` в `ZspFrame::InlineString("SStream(NotImplemented)")`
    #[test]
    fn test_serialize_sstream() {
        let value = Value::SStream(vec![]);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(
            frame,
            ZspFrame::InlineString("SStream(NotImplemented)".into())
        );
    }
}
