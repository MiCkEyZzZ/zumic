use std::{
    collections::HashSet,
    io::{Error, ErrorKind, Read},
};

use byteorder::{BigEndian, ReadBytesExt};
use ordered_float::OrderedFloat;

use super::{
    tags::{TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_NULL, TAG_SET, TAG_STR, TAG_ZSET},
    TAG_BOOL,
};
use crate::{database::DENSE_SIZE, Dict, Hll, Sds, SkipList, SmartHash, Value};

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
        TAG_BOOL => {
            // Булево: 1 => true, 0 => false
            let b = r.read_u8()? != 0;
            Ok(Value::Bool(b))
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
            // читаем ровно n байт (тест может передавать n = 2)
            let mut regs = vec![0u8; n];
            r.read_exact(&mut regs)?;
            // копируем прочитанное в фиксированный буфер HLL.data (DENSE_SIZE),
            // дополняя нулями, если n < DENSE_SIZE
            let mut data = [0u8; DENSE_SIZE];
            data[..n.min(DENSE_SIZE)].copy_from_slice(&regs[..n.min(DENSE_SIZE)]);
            Ok(Value::HyperLogLog(Box::new(Hll { data })))
        }

        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unknown tag {other}"),
        )),
    }
}
