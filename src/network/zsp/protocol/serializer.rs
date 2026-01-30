use std::{borrow::Cow, collections::HashMap};

use super::command::Response;
use crate::{
    zsp::{zsp_types::ZspFrame, PubSubMessage},
    Value,
};

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

        // PubSub ответы
        Response::Message { channel, message } => serialize_pubsub_message(channel, message),
        Response::Subscribed { channel, count } => ZspFrame::Array(vec![
            ZspFrame::InlineString("subscribe".into()),
            ZspFrame::BinaryString(Some(channel.into_bytes())),
            ZspFrame::Integer(count),
        ]),
        Response::Unsubscribed { channel, count } => ZspFrame::Array(vec![
            ZspFrame::InlineString("unsubscribe".into()),
            ZspFrame::BinaryString(Some(channel.into_bytes())),
            ZspFrame::Integer(count),
        ]),
    }
}

// Вспомогательные ф-ии

fn serialize_pubsub_message<'a>(
    channel: String,
    message: PubSubMessage,
) -> ZspFrame<'a> {
    let mut components = vec![
        ZspFrame::InlineString("message".into()),
        ZspFrame::BinaryString(Some(channel.into_bytes())),
    ];

    match message {
        PubSubMessage::Bytes(data) => {
            components.push(ZspFrame::InlineString("BYTES".into()));
            components.push(ZspFrame::BinaryString(Some(data)));
        }
        PubSubMessage::String(s) => {
            components.push(ZspFrame::InlineString("STRING".into()));
            components.push(ZspFrame::BinaryString(Some(s.into_bytes())));
        }
        PubSubMessage::Json(json) => {
            let json_str = serde_json::to_string(&json).unwrap_or_default();
            components.push(ZspFrame::InlineString("JSON".into()));
            components.push(ZspFrame::BinaryString(Some(json_str.into_bytes())));
        }
        PubSubMessage::Serialized { data, content_type } => {
            components.push(ZspFrame::InlineString("SERIALIZED".into()));
            components.push(ZspFrame::BinaryString(Some(content_type.into_bytes())));
            components.push(ZspFrame::BinaryString(Some(data)));
        }
    }

    ZspFrame::Array(components)
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

        // RESP3: Hash → Map (Dictionary)
        Value::Hash(mut smart) => {
            let dict: HashMap<Cow<'a, str>, ZspFrame<'a>> = smart
                .iter()
                .map(|(k, v)| {
                    let key = Cow::from(
                        String::from_utf8(k.to_vec()).unwrap_or_else(|_| "<invalid utf8>".into()),
                    );
                    let val = ZspFrame::BinaryString(Some(v.to_vec()));
                    (key, val)
                })
                .collect();

            ZspFrame::Dictionary(dict)
        }

        // ZSP РАСШИРЕНИЕ: ZSet
        Value::ZSet { dict, .. } => {
            let pairs = dict
                .into_iter()
                .map(|(k_sds, &score)| {
                    let key = String::from_utf8(k_sds.to_vec())
                        .unwrap_or_else(|_| "<invalid utf8>".into());
                    (key, score)
                })
                .collect::<Vec<(String, f64)>>();
            ZspFrame::ZSet(pairs)
        }

        // RESP3: Set → Set (не Array!)
        Value::Set(set) => {
            let frames = set
                .into_iter()
                .map(|item| {
                    let bytes = item.to_vec();
                    match String::from_utf8(bytes.clone()) {
                        Ok(s) if !s.contains('\r') && !s.contains('\n') => {
                            ZspFrame::InlineString(Cow::Owned(s))
                        }
                        _ => ZspFrame::BinaryString(Some(bytes)),
                    }
                })
                .collect();
            ZspFrame::Set(frames)
        }

        Value::Bitmap(bmp) => ZspFrame::BinaryString(Some(bmp.as_bytes().to_vec())),

        Value::HyperLogLog(_) => ZspFrame::InlineString("HLL(NotImplemented)".into()),

        Value::SStream(_) => ZspFrame::InlineString("SStream(NotImplemented)".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{Dict, Hll, QuickList, Sds, SkipList, SmartHash};

    /// Тест проверяет сериализацию `Response::Ok` в
    /// `ZspFrame::InlineString("OK")`
    #[test]
    fn test_serialize_ok() {
        let frame = serialize_response(Response::Ok);
        assert_eq!(frame, ZspFrame::InlineString("OK".into()));
    }

    /// Тест проверяет сериализацию `Response::Error("fail")` в
    /// `ZspFrame::FrameError("fail")`
    #[test]
    fn test_serialize_error() {
        let frame = serialize_response(Response::Error("fail".into()));
        assert_eq!(frame, ZspFrame::FrameError("fail".into()));
    }

    #[test]
    fn test_serialize_null() {
        let frame = serialize_response(Response::NotFound);
        assert_eq!(frame, ZspFrame::Null);
    }

    /// Тест проверяет сериализацию `Value::Str` в
    /// `ZspFrame::BinaryString`
    #[test]
    fn test_serialize_str() {
        let value = Value::Str(Sds::from_str("hello"));
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::BinaryString(Some(b"hello".to_vec())));
    }

    /// Тест проверяет сериализацию `Value::Int` в
    /// `ZspFrame::Integer`
    #[test]
    fn test_serialize_int() {
        let value = Value::Int(123);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::Integer(123));
    }

    /// Тест проверяет сериализацию `Value::Float` в
    /// `ZspFrame::Float`
    #[test]
    fn test_serialize_float() {
        let value = Value::Float(2.14);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::Float(2.14));
    }

    #[test]
    fn test_serialize_bool() {
        let value = Value::Bool(true);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(frame, ZspFrame::Bool(true));
    }

    /// Тест проверяет сериализацию `Value::List` (QuickList) в
    /// `ZspFrame::Array`
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

    #[test]
    fn test_serialize_set_as_resp3_set() {
        let mut set = HashSet::new();
        set.insert(Sds::from_str("x"));
        set.insert(Sds::from_str("y"));
        let value = Value::Set(set);
        let frame = serialize_response(Response::Value(value));

        match frame {
            ZspFrame::Set(set) => {
                assert_eq!(set.len(), 2);
                // Проверяем что это ZspFrame::Set, не Array
            }
            _ => panic!("Expected Set, not Array"),
        }
    }

    #[test]
    fn test_serialize_hash_as_resp3_map() {
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

    #[test]
    fn test_serialize_hll() {
        let hll = Hll::new();
        let value = Value::HyperLogLog(Box::new(hll));
        let frame = serialize_response(Response::Value(value));

        assert_eq!(frame, ZspFrame::InlineString("HLL(NotImplemented)".into()));
    }

    #[test]
    fn test_serialize_sstream() {
        let value = Value::SStream(vec![]);
        let frame = serialize_response(Response::Value(value));
        assert_eq!(
            frame,
            ZspFrame::InlineString("SStream(NotImplemented)".into())
        );
    }

    #[test]
    fn test_serialize_subscribed() {
        let response = Response::Subscribed {
            channel: "news".to_string(),
            count: 1,
        };
        let frame = serialize_response(response);

        match frame {
            ZspFrame::Array(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], ZspFrame::InlineString("subscribe".into()));
                assert_eq!(items[2], ZspFrame::Integer(1));
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_serialize_pubsub_message() {
        let response = Response::Message {
            channel: "news".to_string(),
            message: PubSubMessage::String("hello".to_string()),
        };
        let frame = serialize_response(response);

        match frame {
            ZspFrame::Array(items) => {
                assert_eq!(items[0], ZspFrame::InlineString("message".into()));
                assert_eq!(items[2], ZspFrame::InlineString("STRING".into()));
            }
            _ => panic!("Expected Array"),
        }
    }
}
