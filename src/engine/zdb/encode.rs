use std::io::Write;

use byteorder::{BigEndian, WriteBytesExt};

use super::tags::{
    TAG_BOOL, TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_LIST, TAG_NULL, TAG_SET, TAG_SSTREAM,
    TAG_STR, TAG_ZSET,
};
use crate::Value;

/// Запись Value в поток
pub fn write_value<W: Write>(w: &mut W, v: &Value) -> std::io::Result<()> {
    match v {
        Value::Str(s) => {
            w.write_u8(TAG_STR)?;
            let b = s.as_bytes();
            w.write_u32::<BigEndian>(b.len() as u32)?;
            w.write_all(b)
        }
        Value::Int(i) => {
            w.write_u8(TAG_INT)?;
            w.write_i64::<BigEndian>(*i)
        }
        Value::Float(f) => {
            w.write_u8(TAG_FLOAT)?;
            w.write_f64::<BigEndian>(*f)
        }
        Value::Bool(b) => {
            w.write_u8(TAG_BOOL)?;
            w.write_u8(if *b { 1 } else { 0 })
        }
        Value::Null => w.write_u8(TAG_NULL),
        Value::List(list) => {
            w.write_u8(TAG_LIST)?;
            // считаем длину
            w.write_u32::<BigEndian>(list.len() as u32)?;
            for item in list.iter() {
                write_value(w, &Value::Str(item.clone()))?;
            }
            Ok(())
        }

        Value::Hash(hmap) => {
            w.write_u8(TAG_HASH)?;
            w.write_u32::<BigEndian>(hmap.len() as u32)?;
            // entries() возвращает Vec<(Sds, Sds)>, не требует &mut
            for (field, val) in hmap.entries() {
                // ключ
                let fb = field.as_bytes();
                w.write_u32::<BigEndian>(fb.len() as u32)?;
                w.write_all(fb)?;
                // значение — строка (Sds)
                w.write_u8(TAG_STR)?;
                let vb = val.as_bytes();
                w.write_u32::<BigEndian>(vb.len() as u32)?;
                w.write_all(vb)?;
            }
            Ok(())
        }

        Value::ZSet { dict, sorted } => {
            w.write_u8(TAG_ZSET)?;
            w.write_u32::<BigEndian>(dict.len() as u32)?;
            for (score_wrapper, key) in sorted.iter() {
                let score = score_wrapper.into_inner();
                let kb = key.as_bytes();
                w.write_u32::<BigEndian>(kb.len() as u32)?;
                w.write_all(kb)?;
                w.write_f64::<BigEndian>(score)?;
            }
            Ok(())
        }
        Value::Set(s) => {
            w.write_u8(TAG_SET)?;
            w.write_u32::<BigEndian>(s.len() as u32)?;
            for member in s.iter() {
                let mb = member.as_bytes();
                w.write_u32::<BigEndian>(mb.len() as u32)?;
                w.write_all(mb)?;
            }
            Ok(())
        }
        Value::HyperLogLog(hll) => {
            w.write_u8(TAG_HLL)?;
            w.write_u32::<BigEndian>(hll.data.len() as u32)?;
            w.write_all(&hll.data)
        }
        Value::SStream(entries) => {
            w.write_u8(TAG_SSTREAM)?;
            w.write_u32::<BigEndian>(entries.len() as u32)?;
            for entry in entries {
                // id — теперь два поля: ms_time и sequence
                w.write_u64::<BigEndian>(entry.id.ms_time)?;
                w.write_u64::<BigEndian>(entry.id.sequence)?;
                // поля map<String, Value>
                w.write_u32::<BigEndian>(entry.data.len() as u32)?;
                for (field, val) in entry.data.iter() {
                    // поле — строка
                    let fb = field.as_bytes();
                    w.write_u32::<BigEndian>(fb.len() as u32)?;
                    w.write_all(fb)?;
                    // значение
                    write_value(w, val)?;
                }
            }
            Ok(())
        }
    }
}
