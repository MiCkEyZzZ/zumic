use std::{
    collections::HashSet,
    io::{self, Error, ErrorKind, Read},
};

use byteorder::{BigEndian, ReadBytesExt};
use ordered_float::OrderedFloat;

use super::tags::{TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_NULL, TAG_SET, TAG_STR, TAG_ZSET};
use crate::{database::DENSE_SIZE, Dict, Sds, SkipList, SmartHash, Value, HLL};

/// Чтение Value из потока.
pub fn read_value<R: Read>(r: &mut R) -> std::io::Result<Value> {
    let tag = r.read_u8()?;
    match tag {
        TAG_STR => {
            let len = r.read_u32::<BigEndian>()? as usize;
            let mut buf = vec![0; len];
            r.read_exact(&mut buf)?;
            Ok(Value::Str(Sds::from_vec(buf)))
        }
        TAG_INT => {
            let i = r.read_i64::<BigEndian>()?;
            Ok(Value::Int(i))
        }
        TAG_FLOAT => {
            let f = r.read_f64::<BigEndian>()?;
            Ok(Value::Float(f))
        }
        TAG_NULL => Ok(Value::Null),
        TAG_HASH => {
            let n = r.read_u32::<BigEndian>()? as usize;
            let mut map = SmartHash::new();
            for _ in 0..n {
                // читаем ключ
                let klen = r.read_u32::<BigEndian>()? as usize;
                let mut kb = vec![0; klen];
                r.read_exact(&mut kb)?;
                let key = Sds::from_vec(kb);

                // читаем значение как Value
                let raw = read_value(r)?;
                // проверяем, что Value::Str и берём из него Sds
                let val = if let Value::Str(s) = raw {
                    s
                } else {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Expected Str for Hash value",
                    ));
                };

                map.insert(key, val);
            }
            Ok(Value::Hash(map))
        }
        TAG_ZSET => {
            let n = r.read_u32::<BigEndian>()? as usize;
            let mut dict = Dict::new();
            let mut sorted = SkipList::new();
            for _ in 0..n {
                let klen = r.read_u32::<BigEndian>()? as usize;
                let mut kb = vec![0; klen];
                r.read_exact(&mut kb)?;
                let key = Sds::from_vec(kb);
                let score = r.read_f64::<BigEndian>()?;
                dict.insert(key.clone(), score);
                sorted.insert(OrderedFloat(score), key);
            }
            Ok(Value::ZSet { dict, sorted })
        }
        TAG_SET => {
            let n = r.read_u32::<BigEndian>()? as usize;
            let mut set = HashSet::new();
            for _ in 0..n {
                let klen = r.read_u32::<BigEndian>()? as usize;
                let mut kb = vec![0; klen];
                r.read_exact(&mut kb)?;
                set.insert(Sds::from_vec(kb));
            }
            Ok(Value::Set(set))
        }
        TAG_HLL => {
            let n = r.read_u32::<BigEndian>()? as usize;
            if n != DENSE_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid HLL length {}, expected {}", n, DENSE_SIZE),
                ));
            }
            let mut regs = [0u8; DENSE_SIZE];
            r.read_exact(&mut regs)?;
            Ok(Value::HyperLogLog(HLL { data: regs }))
        }

        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unknown tag {}", other),
        )),
    }
}

#[cfg(test)]
mod tests {

    use std::io::Cursor;

    use byteorder::WriteBytesExt;

    use super::*;

    #[test]
    fn test_read_str() {
        let mut buf = Vec::new();
        buf.write_u8(TAG_STR).unwrap();
        let data = b"hello";
        buf.write_u32::<BigEndian>(data.len() as u32).unwrap();
        buf.extend_from_slice(data);
        let mut cursor = Cursor::new(buf);
        let v = read_value(&mut cursor).unwrap();
        assert_eq!(v, Value::Str(Sds::from_str("hello")));
    }

    #[test]
    fn test_read_int_and_float_and_null() {
        let mut buf = Vec::new();
        buf.write_u8(TAG_INT).unwrap();
        buf.write_i64::<BigEndian>(-42).unwrap();
        buf.write_u8(TAG_FLOAT).unwrap();
        buf.write_f64::<BigEndian>(3.14).unwrap();
        buf.write_u8(TAG_NULL).unwrap();
        let mut cursor = Cursor::new(buf);
        assert_eq!(read_value(&mut cursor).unwrap(), Value::Int(-42));
        assert_eq!(read_value(&mut cursor).unwrap(), Value::Float(3.14));
        assert_eq!(read_value(&mut cursor).unwrap(), Value::Null);
    }

    #[test]
    fn test_read_hash() {
        let mut buf = Vec::new();
        buf.write_u8(TAG_HASH).unwrap();
        buf.write_u32::<BigEndian>(2).unwrap();
        // entry1: key="a", value="1"
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.extend_from_slice(b"a");
        buf.write_u8(TAG_STR).unwrap();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.extend_from_slice(b"1");
        // entry2: key="b", value="2"
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.extend_from_slice(b"b");
        buf.write_u8(TAG_STR).unwrap();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.extend_from_slice(b"2");
        let mut cursor = Cursor::new(buf);
        if let Value::Hash(mut map) = read_value(&mut cursor).unwrap() {
            assert_eq!(map.get(&Sds::from_str("a")).unwrap(), &Sds::from_str("1"));
            assert_eq!(map.get(&Sds::from_str("b")).unwrap(), &Sds::from_str("2"));
        } else {
            panic!("Expected Hash variant");
        }
    }

    #[test]
    fn test_read_zset_set_hll() {
        let mut buf = Vec::new();
        // ZSet: ("x",1.0)
        buf.write_u8(TAG_ZSET).unwrap();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.extend_from_slice(b"x");
        buf.write_f64::<BigEndian>(1.0).unwrap();
        // Set: "y"
        buf.write_u8(TAG_SET).unwrap();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.write_u32::<BigEndian>(1).unwrap();
        buf.extend_from_slice(b"y");
        // HLL: [3,7]
        buf.write_u8(TAG_HLL).unwrap();
        buf.write_u32::<BigEndian>(2).unwrap();
        buf.extend_from_slice(&[3, 7]);

        let mut cursor = Cursor::new(buf);
        // ZSet
        if let Value::ZSet { mut dict, sorted } = read_value(&mut cursor).unwrap() {
            assert_eq!(dict.get(&Sds::from_str("x")), Some(&1.0));
            let (_, key_ref) = sorted.first().unwrap();
            assert_eq!(key_ref, &Sds::from_str("x"));
        } else {
            panic!("Expected ZSet");
        }
        // Set
        if let Value::Set(s) = read_value(&mut cursor).unwrap() {
            assert!(s.contains(&Sds::from_str("y")));
        } else {
            panic!("Expected Set");
        }
        // HLL
        if let Value::HyperLogLog(HLL { data }) = read_value(&mut cursor).unwrap() {
            // only check the first two registers that we wrote
            assert_eq!(&data[..2], &[3, 7]);
        } else {
            panic!("Expected HLL");
        }
    }

    #[test]
    fn test_unknown_tag() {
        let buf = vec![0xFF];
        let mut cursor = Cursor::new(buf);
        let e = read_value(&mut cursor).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InvalidData);
    }
}
