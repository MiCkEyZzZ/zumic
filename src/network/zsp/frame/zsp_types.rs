//! Типы ZSP (Zumic Serialization Protocol).
//!
//! Протокол ZSP — это текстово-бинарный протокол с расширенным
//! набором типов. Здесь определён enum `ZSPFrame<'a>`, а также
//! конвертации из внутренних типов `Value`, `Sds`, `SmartHash`
//! и т. д.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use crate::{Dict, QuickList, Sds, SmartHash, Value};

/// Типы фреймов, поддерживаемые протоколом ZSP.
///
/// Включает в себя различные виды данных, которые могут быть
/// переданы в рамках протокола: быть переданы в рамках протокола:
/// - Простые строки
/// - Ошибки
/// - Целые числа
/// - Двоичные строки
/// - Массивы и словари
#[derive(Debug, Clone, PartialEq)]
pub enum ZSPFrame<'a> {
    InlineString(Cow<'a, str>),
    FrameError(String),
    Integer(i64),
    Float(f64),
    BinaryString(Option<Vec<u8>>),
    Array(Vec<ZSPFrame<'a>>),
    Dictionary(HashMap<Cow<'a, str>, ZSPFrame<'a>>),
    ZSet(Vec<(String, f64)>),
    Null,
}

impl<'a> TryFrom<Value> for ZSPFrame<'a> {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Str(s) => convert_sds_to_frame(s),
            Value::Int(i) => Ok(Self::Integer(i)),
            Value::Float(f) => Ok(Self::Float(f)),
            Value::List(list) => convert_quicklist(list),
            Value::Set(set) => convert_hashset(set),
            // Теперь Value::Hash сохраняет SmartHash, поэтому вызываем новую конвертацию.
            Value::Hash(smart_hash) => convert_smart_hash(smart_hash),
            Value::ZSet { dict, .. } => convert_zset(dict),
            Value::Null => Ok(ZSPFrame::Null),
            // Игнорировать неподдерживаемые типы
            Value::HyperLogLog(_) | Value::SStream(_) => Err("Unsupported data type".into()),
        }
    }
}

impl<'a> From<Sds> for ZSPFrame<'a> {
    fn from(value: Sds) -> Self {
        ZSPFrame::BinaryString(Some(value.to_vec()))
    }
}

pub fn convert_sds_to_frame<'a>(sds: Sds) -> Result<ZSPFrame<'a>, String> {
    let bytes = sds.as_ref();
    match std::str::from_utf8(bytes) {
        Ok(valid_str) => Ok(ZSPFrame::InlineString(Cow::Owned(valid_str.to_string()))),
        Err(_) => Ok(ZSPFrame::BinaryString(Some(bytes.to_vec()))),
    }
}

pub fn convert_quicklist<'a>(list: QuickList<Sds>) -> Result<ZSPFrame<'a>, String> {
    let mut frames = Vec::with_capacity(list.len());
    for item in list.iter() {
        frames.push(item.clone().into());
    }
    Ok(ZSPFrame::Array(frames))
}

pub fn convert_hashset<'a>(set: HashSet<Sds>) -> Result<ZSPFrame<'a>, String> {
    let mut frames = Vec::with_capacity(set.len());
    for item in set {
        frames.push(convert_sds_to_frame(item)?);
    }
    Ok(ZSPFrame::Array(frames))
}

/// Новая функция для конвертации SmartHash в ZSPFrame::Dictionary
#[inline]
pub fn convert_smart_hash<'a>(mut smart: SmartHash) -> Result<ZSPFrame<'a>, String> {
    let mut map = HashMap::with_capacity(smart.len());
    // Используем итератор, предоставляемый SmartHash
    for (k, v) in smart.iter() {
        let key = String::from_utf8(k.to_vec()).map_err(|e| format!("Invalid hash key: {}", e))?;
        let key_cow: Cow<'a, str> = Cow::Owned(key);
        let frame = v.clone().into();
        map.insert(key_cow, frame);
    }
    Ok(ZSPFrame::Dictionary(map))
}

#[inline]
pub fn convert_zset<'a>(dict: Dict<Sds, f64>) -> Result<ZSPFrame<'a>, String> {
    let mut pairs = Vec::with_capacity(dict.len());
    for (k_sds, &score) in dict.iter() {
        let key =
            String::from_utf8(k_sds.to_vec()).map_err(|e| format!("ZSet key error: {}", e))?;
        pairs.push((key, score));
    }
    Ok(ZSPFrame::ZSet(pairs))
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::database::Sds;

    /// Пример теста для проверки конвертации Value::Hash (теперь с SmartHash)
    #[test]
    fn test_convert_smart_hash() {
        // Создаем SmartHash с несколькими записями.
        let mut sh = SmartHash::new();
        sh.insert(Sds::from_str("key1"), Sds::from_str("val1"));
        sh.insert(Sds::from_str("key2"), Sds::from_str("val2"));

        let frame = convert_smart_hash(sh).unwrap();
        if let ZSPFrame::Dictionary(dict) = frame {
            assert_eq!(
                dict.get("key1"),
                Some(&ZSPFrame::BinaryString(Some(b"val1".to_vec())))
            );
            assert_eq!(
                dict.get("key2"),
                Some(&ZSPFrame::BinaryString(Some(b"val2".to_vec())))
            );
        } else {
            panic!("Expected Dictionary frame");
        }
    }

    /// Тестирует обработку Sds как с допустимыми данными UTF-8, так и с двоичными данными.
    #[test]
    fn handle_sds_utf8_and_binary() {
        let utf8 = Sds::from_str("hello");
        let frame = convert_sds_to_frame(utf8).unwrap();
        assert_eq!(frame, ZSPFrame::InlineString("hello".into()));

        let bin = Sds::from_vec(vec![0xFF, 0xFE]);
        let frame = convert_sds_to_frame(bin.clone()).unwrap();
        assert_eq!(frame, ZSPFrame::BinaryString(Some(bin.to_vec())));
    }

    /// Тестирует преобразование QuickList<Sds> в ZSPFrame::Array BinaryStrings.
    #[test]
    fn convert_quicklist_to_array() {
        let mut ql = QuickList::new(16);
        ql.push_back(Sds::from_str("a"));
        ql.push_back(Sds::from_str("b"));

        let zsp = convert_quicklist(ql).unwrap();
        if let ZSPFrame::Array(vec) = zsp {
            let strs: Vec<_> = vec
                .into_iter()
                .map(|f| {
                    if let ZSPFrame::BinaryString(Some(b)) = f {
                        String::from_utf8(b).unwrap()
                    } else {
                        panic!("Expected BinaryString");
                    }
                })
                .collect();
            assert_eq!(strs, vec!["a", "b"]);
        } else {
            panic!("Expected Array frame");
        }
    }

    /// Тестирует преобразование HashSet<String> в ZSPFrame::Array InlineStrings.
    #[test]
    fn convert_hashset_order_independent() {
        let mut hs = HashSet::new();
        hs.insert(Sds::from_str("x"));
        hs.insert(Sds::from_str("y"));
        let zsp = convert_hashset(hs).unwrap();
        if let ZSPFrame::Array(vec) = zsp {
            let mut got: Vec<_> = vec
                .into_iter()
                .map(|f| match f {
                    ZSPFrame::InlineString(Cow::Borrowed(s)) => s.to_string(),
                    ZSPFrame::InlineString(Cow::Owned(s)) => s,
                    ZSPFrame::BinaryString(Some(b)) => String::from_utf8(b).unwrap(),
                    _ => panic!(),
                })
                .collect();
            got.sort();
            assert_eq!(got, vec!["x".to_string(), "y".to_string()]);
        } else {
            panic!("Expected Array frame");
        }
    }

    /// Тестирует TryFrom<Value> для ZSPFrame с различными типами, такими как Int и Null.
    #[test]
    fn try_from_value_various() {
        assert_eq!(
            ZSPFrame::try_from(Value::Int(10)).unwrap(),
            ZSPFrame::Integer(10)
        );
        assert_eq!(ZSPFrame::try_from(Value::Null).unwrap(), ZSPFrame::Null);
    }

    /// Тестирует преобразование ZSet (HashMap<Sds, f64>) в ZSPFrame::ZSet.
    #[test]
    fn convert_zset_to_frame() {
        let mut zs = Dict::new();
        zs.insert(Sds::from_str("foo"), 1.1);
        zs.insert(Sds::from_str("bar"), 2.2);

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

    /// Проверяет TryFrom<Value::Str> как на допустимые, так и на недопустимые UTF-8 Sds.
    #[test]
    fn try_from_value_str_valid_and_invalid_utf8() {
        let valid = Sds::from_str("abc");
        let invalid = Sds::from_vec(vec![0xFF, 0xFE]);

        assert_eq!(
            ZSPFrame::try_from(Value::Str(valid.clone())).unwrap(),
            ZSPFrame::InlineString("abc".into())
        );

        let frame = ZSPFrame::try_from(Value::Str(invalid.clone())).unwrap();
        assert_eq!(frame, ZSPFrame::BinaryString(Some(invalid.to_vec())));
    }

    /// Тестовое преобразование пустого Quicklist в пустой фрейм массива.
    #[test]
    fn test_empty_quicklist() {
        let ql = QuickList::new(16);
        let zsp = convert_quicklist(ql).unwrap();
        assert_eq!(zsp, ZSPFrame::Array(vec![]));
    }

    /// Тестовое преобразование пустого HashSet в пустой фрейм массива.
    #[test]
    fn convert_empty_hashset() {
        let hs = HashSet::new();
        let zsp = convert_hashset(hs).unwrap();
        assert_eq!(zsp, ZSPFrame::Array(vec![]));
    }

    /// Тестовое преобразование пустого HashMap в пустой фрейм словаря.
    #[test]
    fn convert_empty_hashmap() {
        let hm: HashMap<Sds, Sds> = HashMap::new();
        let zsp = convert_smart_hash(SmartHash::from_iter(hm)).unwrap();
        assert_eq!(zsp, ZSPFrame::Dictionary(HashMap::new()));
        assert_eq!(zsp, ZSPFrame::Dictionary(HashMap::new()));
    }

    /// Проверьте, что преобразование HashMap с недопустимым ключом UTF-8 возвращает ошибку.
    #[test]
    fn convert_hashmap_with_invalid_utf8_key() {
        let mut hm = HashMap::new();
        hm.insert(Sds::from_vec(vec![0xFF]), Sds::from_str("val"));

        let err = convert_smart_hash(SmartHash::from_iter(hm)).unwrap_err();
        assert!(err.contains("Invalid hash key"));
    }

    /// Проверьте, что преобразование ZSet с недопустимым ключом UTF-8 возвращает ошибку.
    #[test]
    fn convert_zset_with_invalid_utf8_key() {
        let mut zs = Dict::new();
        zs.insert(Sds::from_vec(vec![0xFF]), 1.0);

        let err = convert_zset(zs).unwrap_err();
        assert!(err.contains("ZSet key error"));
    }

    /// Проверяем, что Sds преобразуется в BinaryString с помощью `From` impl.
    #[test]
    fn arcbytes_into_binarytring() {
        let arc = Sds::from_str("hello");
        let frame: ZSPFrame<'_> = arc.clone().into();
        assert_eq!(frame, ZSPFrame::BinaryString(Some(arc.to_vec())));
    }
}
