use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use crate::database::{ArcBytes, QuickList, Value};

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
}

impl TryFrom<Value> for ZSPFrame {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Str(s) => handle_arcbytes(s),
            Value::Int(i) => Ok(Self::Integer(i)),
            Value::Float(f) => Ok(Self::Float(f)),
            Value::Bool(b) => Ok(Self::SimpleString(b.to_string())),
            Value::List(list) => convert_quicklist(list),
            Value::Set(set) => convert_hashset(set),
            Value::Hash(hash) => convert_hashmap(hash),
            Value::ZSet { dict, .. } => convert_zset(dict),
            Value::Null => Ok(Self::BulkString(None)),
            // Игнорируем неподдерживаемые типы
            Value::HyperLogLog(_) | Value::SStream(_) => Err("Unsupported data type".into()),
        }
    }
}

impl From<ArcBytes> for ZSPFrame {
    fn from(value: ArcBytes) -> Self {
        ZSPFrame::BulkString(Some(value.to_vec()))
    }
}

fn handle_arcbytes(bytes: ArcBytes) -> Result<ZSPFrame, String> {
    String::from_utf8(bytes.to_vec())
        .map(ZSPFrame::SimpleString)
        .or_else(|_| Ok(ZSPFrame::BulkString(Some(bytes.to_vec()))))
}

fn convert_quicklist(list: QuickList<ArcBytes>) -> Result<ZSPFrame, String> {
    let mut frames = Vec::with_capacity(list.len());
    for item in list.iter() {
        frames.push(item.clone().into());
    }
    Ok(ZSPFrame::Array(Some(frames)))
}

fn convert_hashset(set: HashSet<String>) -> Result<ZSPFrame, String> {
    Ok(ZSPFrame::Array(Some(
        set.into_iter().map(ZSPFrame::SimpleString).collect(),
    )))
}

fn convert_hashmap(hash: HashMap<ArcBytes, ArcBytes>) -> Result<ZSPFrame, String> {
    let mut map = HashMap::with_capacity(hash.len());
    for (k, v) in hash {
        // 1) decode the key, mapping its UTF‑8 error into String:
        let key = String::from_utf8(k.to_vec()).map_err(|e| format!("Invalid hash key: {}", e))?;
        // 2) infallibly convert the value:
        let frame = v.into();
        map.insert(key, frame);
    }
    Ok(ZSPFrame::Dictionary(Some(map)))
}

fn convert_zset(dict: HashMap<ArcBytes, f64>) -> Result<ZSPFrame, String> {
    // Turn each (ArcBytes, f64) into a (String, f64), mapping UTF‑8 errors into String
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

    use crate::database::{ArcBytes, QuickList};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn handle_arcbytes_utf8_and_binary() {
        let utf8 = ArcBytes::from_str("hello");
        let frame = handle_arcbytes(utf8).unwrap();
        assert_eq!(frame, ZSPFrame::SimpleString("hello".into()));

        let bin = ArcBytes::from(vec![0xFF, 0xFE]);
        let frame = handle_arcbytes(bin.clone()).unwrap();
        assert_eq!(frame, ZSPFrame::BulkString(Some(bin.to_vec())));
    }

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
            panic!()
        }
    }

    #[test]
    fn try_from_value_various() {
        assert_eq!(
            ZSPFrame::try_from(Value::Int(10)).unwrap(),
            ZSPFrame::Integer(10)
        );
        assert_eq!(
            ZSPFrame::try_from(Value::Null).unwrap(),
            ZSPFrame::BulkString(None)
        );
    }

    #[test]
    fn convert_hashmap_to_dict() {
        let mut hm = HashMap::new();
        hm.insert(ArcBytes::from_str("key1"), ArcBytes::from_str("val1"));
        hm.insert(ArcBytes::from_str("key2"), ArcBytes::from_str("val2"));

        let zsp = convert_hashmap(hm).unwrap();
        if let ZSPFrame::Dictionary(Some(map)) = zsp {
            assert_eq!(
                map.get("key1"),
                Some(&ZSPFrame::BulkString(Some(b"val1".to_vec())))
            );
            assert_eq!(
                map.get("key2"),
                Some(&ZSPFrame::BulkString(Some(b"val2".to_vec())))
            );
        } else {
            panic!("Expected Dictionary frame");
        }
    }

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

    #[test]
    fn try_from_value_bool_and_float() {
        assert_eq!(
            ZSPFrame::try_from(Value::Bool(true)).unwrap(),
            ZSPFrame::SimpleString("true".to_string())
        );
        assert_eq!(
            ZSPFrame::try_from(Value::Float(3.14)).unwrap(),
            ZSPFrame::Float(3.14)
        );
    }

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
}
