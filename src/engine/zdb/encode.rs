use std::io::Write;

use byteorder::{BigEndian, WriteBytesExt};

use super::tags::{
    TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_LIST, TAG_NULL, TAG_SET, TAG_SSTREAM, TAG_STR,
    TAG_ZSET,
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
            w.write_u32::<BigEndian>(hll.registers.len() as u32)?;
            w.write_all(&hll.registers)
        }
        Value::SStream(entries) => {
            w.write_u8(TAG_SSTREAM)?;
            w.write_u32::<BigEndian>(entries.len() as u32)?;
            for entry in entries {
                // id
                w.write_u64::<BigEndian>(entry.id)?;
                // поля map<String,Value>
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        io::{Cursor, Read},
    };

    use byteorder::ReadBytesExt;
    use ordered_float::OrderedFloat;

    use crate::{Dict, QuickList, Sds, SkipList, SmartHash};

    use super::*;

    fn create_quicklist_from_vec(vec: Vec<String>, max_segment_size: usize) -> QuickList<Sds> {
        let sds_vec: Vec<Sds> = vec.into_iter().map(|s| Sds::from_str(&s)).collect();
        QuickList::from_iter(sds_vec, max_segment_size)
    }

    #[test]
    fn test_write_str() {
        let value = Value::Str(Sds::from_str("hello"));
        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_STR);

        let len = cursor.read_u32::<BigEndian>().unwrap() as usize;
        let mut buf = vec![0; len];
        cursor.read_exact(&mut buf).unwrap();
        let result_str = String::from_utf8(buf).unwrap();
        assert_eq!(result_str, "hello");
    }

    #[test]
    fn test_write_int() {
        let value = Value::Int(42);
        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_INT);

        let result = cursor.read_i64::<BigEndian>().unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_write_float() {
        let value = Value::Float(3.14);
        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_FLOAT);

        let result = cursor.read_f64::<BigEndian>().unwrap();
        assert_eq!(result, 3.14);
    }

    #[test]
    fn test_write_null() {
        let value = Value::Null;
        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_NULL);
    }

    #[test]
    fn test_write_list() {
        let vec = vec!["a".to_string(), "b".to_string()];
        let max_segment_size = 2;

        let list = create_quicklist_from_vec(vec, max_segment_size);
        let value = Value::List(list);
        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_LIST);

        let len = cursor.read_u32::<BigEndian>().unwrap() as usize;
        assert_eq!(len, 2);

        for _ in 0..len {
            let tag = cursor.read_u8().unwrap();
            assert_eq!(tag, TAG_STR);
            let len = cursor.read_u32::<BigEndian>().unwrap() as usize;
            let mut buf = vec![0; len];
            cursor.read_exact(&mut buf).unwrap();
            let result_str = String::from_utf8(buf).unwrap();
            assert!(result_str == "a" || result_str == "b");
        }
    }

    #[test]
    fn test_write_smart_hash() {
        let mut smart_hash = SmartHash::new();
        smart_hash.insert(Sds::from_str("field1"), Sds::from_str("value1"));
        smart_hash.insert(Sds::from_str("field2"), Sds::from_str("value2"));

        let value = Value::Hash(smart_hash);
        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_HASH);

        let len = cursor.read_u32::<BigEndian>().unwrap() as usize;
        assert_eq!(len, 2);

        for _ in 0..len {
            let field_len = cursor.read_u32::<BigEndian>().unwrap() as usize;
            let mut field_buf = vec![0; field_len];
            cursor.read_exact(&mut field_buf).unwrap();
            let field = String::from_utf8(field_buf).unwrap();

            let value_tag = cursor.read_u8().unwrap();
            assert_eq!(value_tag, TAG_STR);

            let value_len = cursor.read_u32::<BigEndian>().unwrap() as usize;
            let mut value_buf = vec![0; value_len];
            cursor.read_exact(&mut value_buf).unwrap();
            let value = String::from_utf8(value_buf).unwrap();

            assert!(field == "field1" || field == "field2");
            assert!(value == "value1" || value == "value2");
        }
    }

    #[test]
    fn test_write_zset() {
        // Создаем ZSet с использованием твоего Dict для хранения элементов и их баллов
        let mut dict = Dict::new();
        dict.insert(Sds::from_str("member1"), 1.0);
        dict.insert(Sds::from_str("member2"), 2.0);

        // Создаем SkipList для сортированных элементов
        let mut sorted = SkipList::new();
        sorted.insert(OrderedFloat(1.0), Sds::from_str("member1"));
        sorted.insert(OrderedFloat(2.0), Sds::from_str("member2"));

        // Создаем ZSet с помощью Dict и SkipList
        let value = Value::ZSet { dict, sorted };

        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_ZSET);

        let len = cursor.read_u32::<BigEndian>().unwrap() as usize;
        assert_eq!(len, 2);

        for _ in 0..len {
            let member_len = cursor.read_u32::<BigEndian>().unwrap() as usize;
            let mut member_buf = vec![0; member_len];
            cursor.read_exact(&mut member_buf).unwrap();
            let member = String::from_utf8(member_buf).unwrap();

            let score = cursor.read_f64::<BigEndian>().unwrap();
            assert!(member == "member1" || member == "member2");
            assert!(score == 1.0 || score == 2.0);
        }
    }

    #[test]
    fn test_write_set() {
        let set = HashSet::from([Sds::from_str("a"), Sds::from_str("b")]);
        let value = Value::Set(set);
        let mut buf = Vec::new();
        write_value(&mut buf, &value).unwrap();

        let mut cursor = Cursor::new(buf);
        let tag = cursor.read_u8().unwrap();
        assert_eq!(tag, TAG_SET);

        let len = cursor.read_u32::<BigEndian>().unwrap() as usize;
        assert_eq!(len, 2);

        for _ in 0..len {
            let member_len = cursor.read_u32::<BigEndian>().unwrap() as usize;
            let mut member_buf = vec![0; member_len];
            cursor.read_exact(&mut member_buf).unwrap();
            let member = String::from_utf8(member_buf).unwrap();
            assert!(member == "a" || member == "b");
        }
    }
}
