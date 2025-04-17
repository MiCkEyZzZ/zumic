use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use tracing::{debug, warn};

use crate::database::{ArcBytes, QuickList, SmartHash, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum ZSPFrame {
    SimpleString(String),
    FrameError(String),
    Integer(i64),
    Float(f64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<ZSPFrame>>),
    Dictionary(Option<HashMap<String, ZSPFrame>>),
    ZSet(Vec<(String, f64)>),
    Null,
}

impl TryFrom<Value> for ZSPFrame {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        debug!("Attempting to convert Value to ZSPFrame: {:?}", value);
        match value {
            Value::Str(s) => {
                debug!("Converting Value::Str to ZSPFrame::SimpleString");
                convert_arcbytes_to_frame(s)
            }
            Value::Int(i) => {
                debug!("Converting Value::Int to ZSPFrame::Integer: {}", i);
                Ok(Self::Integer(i))
            }
            Value::Float(f) => {
                debug!("Converting Value::Float to ZSPFrame::Float: {}", f);
                Ok(Self::Float(f))
            }
            Value::List(list) => {
                debug!("Converting Value::List to ZSPFrame::Array");
                convert_quicklist(list)
            }
            Value::Set(set) => {
                debug!("Converting Value::Set to ZSPFrame::Array");
                convert_hashset(set)
            }
            // Теперь Value::Hash сохраняет SmartHash, поэтому вызываем новую конвертацию.
            Value::Hash(smart_hash) => {
                debug!("Converting Value::Hash (SmartHash) to ZSPFrame::Dictionary");
                convert_smart_hash(smart_hash)
            }
            Value::ZSet { dict, .. } => {
                debug!("Converting Value::ZSet to ZSPFrame::ZSet");
                convert_zset(dict)
            }
            Value::Null => {
                debug!("Converting Value::Null to ZSPFrame::Null");
                Ok(ZSPFrame::Null)
            }
            // Игнорировать неподдерживаемые типы
            Value::HyperLogLog(_) | Value::SStream(_) => {
                warn!("Unsupported data type encountered during conversion");
                Err("Unsupported data type".into())
            }
        }
    }
}

impl From<ArcBytes> for ZSPFrame {
    fn from(value: ArcBytes) -> Self {
        debug!("Converting ArcBytes to ZSPFrame::BulkString");
        ZSPFrame::BulkString(Some(value.to_vec()))
    }
}

fn convert_arcbytes_to_frame(bytes: ArcBytes) -> Result<ZSPFrame, String> {
    debug!("Handling ArcBytes: {:?}", bytes);
    String::from_utf8(bytes.to_vec())
        .map(ZSPFrame::SimpleString)
        .or_else(|_| {
            debug!("Non-UTF8 ArcBytes, converting to BulkString");
            Ok(ZSPFrame::BulkString(Some(bytes.to_vec())))
        })
}

fn convert_quicklist(list: QuickList<ArcBytes>) -> Result<ZSPFrame, String> {
    debug!(
        "Converting QuickList to ZSPFrame::Array with length: {}",
        list.len()
    );
    let mut frames = Vec::with_capacity(list.len());
    for item in list.iter() {
        frames.push(item.clone().into());
    }
    Ok(ZSPFrame::Array(Some(frames)))
}

fn convert_hashset(set: HashSet<String>) -> Result<ZSPFrame, String> {
    debug!("Converting HashSet to ZSPFrame::Array");
    Ok(ZSPFrame::Array(Some(
        set.into_iter().map(ZSPFrame::SimpleString).collect(),
    )))
}

/// Новая функция для конвертации SmartHash в ZSPFrame::Dictionary
fn convert_smart_hash(smart: SmartHash) -> Result<ZSPFrame, String> {
    debug!("Converting SmartHash to ZSPFrame::Dictionary");
    let mut map = HashMap::with_capacity(smart.len());
    // Используем итератор, предоставляемый SmartHash
    for (k, v) in smart.iter() {
        let key = String::from_utf8(k.to_vec()).map_err(|e| format!("Invalid hash key: {}", e))?;
        let frame = v.clone().into();
        map.insert(key, frame);
    }
    Ok(ZSPFrame::Dictionary(Some(map)))
}

fn convert_zset(dict: HashMap<ArcBytes, f64>) -> Result<ZSPFrame, String> {
    debug!("Converting HashMap (ZSet) to ZSPFrame::ZSet");
    let pairs = dict
        .into_iter()
        .map(|(k, score)| {
            let key =
                String::from_utf8(k.to_vec()).map_err(|e| format!("ZSet key error: {}", e))?;
            Ok((key, score))
        })
        .collect::<Result<Vec<(String, f64)>, String>>()?;
    Ok(ZSPFrame::ZSet(pairs))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::{HashMap, HashSet};

    // Пример теста для проверки конвертации Value::Hash (теперь с SmartHash)
    #[test]
    fn test_convert_smart_hash() {
        // Создаем SmartHash с несколькими записями.
        let mut sh = SmartHash::new();
        sh.hset(ArcBytes::from_str("key1"), ArcBytes::from_str("val1"));
        sh.hset(ArcBytes::from_str("key2"), ArcBytes::from_str("val2"));

        let frame = convert_smart_hash(sh).unwrap();
        if let ZSPFrame::Dictionary(Some(dict)) = frame {
            assert_eq!(
                dict.get("key1"),
                Some(&ZSPFrame::BulkString(Some(b"val1".to_vec())))
            );
            assert_eq!(
                dict.get("key2"),
                Some(&ZSPFrame::BulkString(Some(b"val2".to_vec())))
            );
        } else {
            panic!("Expected Dictionary frame");
        }
    }

    // Tests handling of ArcBytes with both valid UTF-8 and binary data.
    #[test]
    fn handle_arcbytes_utf8_and_binary() {
        let utf8 = ArcBytes::from_str("hello");
        let frame = convert_arcbytes_to_frame(utf8).unwrap();
        assert_eq!(frame, ZSPFrame::SimpleString("hello".into()));

        let bin = ArcBytes::from(vec![0xFF, 0xFE]);
        let frame = convert_arcbytes_to_frame(bin.clone()).unwrap();
        assert_eq!(frame, ZSPFrame::BulkString(Some(bin.to_vec())));
    }

    // Tests the conversion of QuickList<ArcBytes> into a ZSPFrame::Array of BulkStrings.
    #[test]
    fn convert_quicklist_to_array() {
        let mut ql = QuickList::new(16);
        ql.push_back(ArcBytes::from_str("a"));
        ql.push_back(ArcBytes::from_str("b"));

        let zsp = convert_quicklist(ql).unwrap();
        if let ZSPFrame::Array(Some(vec)) = zsp {
            let strs: Vec<_> = vec
                .into_iter()
                .map(|f| {
                    if let ZSPFrame::BulkString(Some(b)) = f {
                        String::from_utf8(b).unwrap()
                    } else {
                        panic!("Expected BulkString");
                    }
                })
                .collect();
            assert_eq!(strs, vec!["a", "b"]);
        } else {
            panic!("Expected Array frame");
        }
    }

    // Tests the conversion of a HashSet<String> to a ZSPFrame::Array of SimpleStrings.
    #[test]
    fn convert_hashset_order_independent() {
        let mut hs = HashSet::new();
        hs.insert("x".to_string());
        hs.insert("y".to_string());
        let zsp = convert_hashset(hs).unwrap();
        if let ZSPFrame::Array(Some(vec)) = zsp {
            let mut got: Vec<_> = vec
                .into_iter()
                .map(|f| {
                    if let ZSPFrame::SimpleString(s) = f {
                        s
                    } else {
                        panic!()
                    }
                })
                .collect();
            got.sort();
            assert_eq!(got, vec!["x".to_string(), "y".to_string()]);
        } else {
            panic!("Expected Array frame");
        }
    }

    // Tests TryFrom<Value> for ZSPFrame with various types like Int and Null.
    #[test]
    fn try_from_value_various() {
        assert_eq!(
            ZSPFrame::try_from(Value::Int(10)).unwrap(),
            ZSPFrame::Integer(10)
        );
        assert_eq!(ZSPFrame::try_from(Value::Null).unwrap(), ZSPFrame::Null);
    }

    // Tests conversion of a ZSet (HashMap<ArcBytes, f64>) into a ZSPFrame::ZSet.
    #[test]
    fn convert_zset_to_frame() {
        let mut zs = HashMap::new();
        zs.insert(ArcBytes::from_str("foo"), 1.1);
        zs.insert(ArcBytes::from_str("bar"), 2.2);

        let result = convert_zset(zs).unwrap();
        if let ZSPFrame::ZSet(mut pairs) = result {
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            assert_eq!(
                pairs,
                vec![("bar".to_string(), 2.2), ("foo".to_string(), 1.1)]
            );
        } else {
            panic!("Expected ZSet frame");
        }
    }

    // Tests TryFrom<Value::Str> for both valid UTF-8 and invalid UTF-8 ArcBytes.
    #[test]
    fn try_from_value_str_valid_and_invalid_utf8() {
        let valid = ArcBytes::from_str("abc");
        let invalid = ArcBytes::from(vec![0xFF, 0xFE]);

        assert_eq!(
            ZSPFrame::try_from(Value::Str(valid.clone())).unwrap(),
            ZSPFrame::SimpleString("abc".into())
        );

        let frame = ZSPFrame::try_from(Value::Str(invalid.clone())).unwrap();
        assert_eq!(frame, ZSPFrame::BulkString(Some(invalid.to_vec())));
    }

    // Test conversion of an empty Quicklist into an empty Array frame.
    #[test]
    fn test_empty_quicklist() {
        let ql = QuickList::new(16);
        let zsp = convert_quicklist(ql).unwrap();
        assert_eq!(zsp, ZSPFrame::Array(Some(vec![])));
    }

    // Test conversion of an empty HashSet into an empty Array frame.
    #[test]
    fn convert_empty_hashset() {
        let hs = HashSet::new();
        let zsp = convert_hashset(hs).unwrap();
        assert_eq!(zsp, ZSPFrame::Array(Some(vec![])));
    }

    // Test conversion of an empty HashMap into an empty Dictionary frame.
    #[test]
    fn convert_empty_hashmap() {
        let hm: HashMap<ArcBytes, ArcBytes> = HashMap::new();
        let zsp = convert_smart_hash(SmartHash::from_iter(hm.into_iter())).unwrap();
        assert_eq!(zsp, ZSPFrame::Dictionary(Some(HashMap::new())));
    }

    // Test that converting a HashMap with an invalid UTF-8 key returns an error.
    #[test]
    fn convert_hashmap_with_invalid_utf8_key() {
        let mut hm = HashMap::new();
        hm.insert(ArcBytes::from(vec![0xFF]), ArcBytes::from_str("val"));

        let err = convert_smart_hash(SmartHash::from_iter(hm.into_iter())).unwrap_err();
        assert!(err.contains("Invalid hash key"));
    }

    // Test that converting a ZSet with an invalid UTF-8 key returns an error.
    #[test]
    fn convert_zset_with_invalid_utf8_key() {
        let mut zs = HashMap::new();
        zs.insert(ArcBytes::from(vec![0xFF]), 1.0);

        let err = convert_zset(zs).unwrap_err();
        assert!(err.contains("ZSet key error"));
    }

    // Test that ArcBytes is converted into a BulkString using `From` impl.
    #[test]
    fn arcbytes_into_bulkstring() {
        let arc = ArcBytes::from_str("hello");
        let frame: ZSPFrame = arc.clone().into();
        assert_eq!(frame, ZSPFrame::BulkString(Some(arc.to_vec())));
    }
}
