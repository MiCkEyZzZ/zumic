use std::{
    collections::{HashMap, HashSet},
    io::{Error, ErrorKind, Read},
};

use byteorder::{BigEndian, ReadBytesExt};
use ordered_float::OrderedFloat;

use super::tags::{
    TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_NULL, TAG_SET, TAG_SSTREAM, TAG_STR, TAG_ZSET,
};
use crate::{
    database::types::{StreamEntry, HLL},
    Dict, Sds, SkipList, SmartHash, Value,
};

/// Чтение Value из потока.
pub fn read_value<R: Read>(r: &mut R) -> std::io::Result<Value> {
    let tag = r.read_u8()?;
    match tag {
        TAG_STR => {
            let len = r.read_u32::<BigEndian>()? as usize;
            let buf = vec![0; len];
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
            let mut regs = vec![0; n];
            r.read_exact(&mut regs)?;
            Ok(Value::HyperLogLog(HLL { registers: regs }))
        }
        TAG_SSTREAM => {
            let n = r.read_u32::<BigEndian>()? as usize;
            let mut stream = Vec::with_capacity(n);
            for _ in 0..n {
                let id = r.read_u64::<BigEndian>()?;
                let m = r.read_u32::<BigEndian>()? as usize;
                let mut data = HashMap::new();
                for _ in 0..m {
                    let flen = r.read_u32::<BigEndian>()? as usize;
                    let mut fb = vec![0; flen];
                    r.read_exact(&mut fb)?;
                    let field = String::from_utf8(fb).unwrap();
                    let val = read_value(r)?;
                    data.insert(field, val);
                }
                stream.push(StreamEntry { id, data });
            }
            Ok(Value::SStream(stream))
        }
        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unknown tag {}", other),
        )),
    }
}
