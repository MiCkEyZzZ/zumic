//! Модуль для сериализации значений `Value` в бинарный формат.
//!
//! Все типы значений кодируются с префиксным тегом,
//! за которым следует длина и содержимое (если применимо).
//!
//! Используется BigEndian-формат для чисел.

use std::io::Write;

use byteorder::{BigEndian, WriteBytesExt};

use super::{
    compress_block, should_compress, DUMP_VERSION, FILE_MAGIC, TAG_BOOL, TAG_COMPRESSED, TAG_EOF,
    TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_LIST, TAG_NULL, TAG_SET, TAG_SSTREAM, TAG_STR,
    TAG_ZSET,
};
use crate::{Sds, Value};

/// Сериализует значение [`Value`] в поток `Write` с автоматическим сжатием.
pub fn write_value<W: Write>(
    w: &mut W,
    v: &Value,
) -> std::io::Result<()> {
    // Сначала сериализуем значение во внутренний буфер
    let mut buf = Vec::new();
    write_value_inner(&mut buf, v)?;

    // Если буфер большой — сжимаем его
    if should_compress(buf.len()) {
        let compressed = compress_block(&buf)?;
        w.write_u8(TAG_COMPRESSED)?;
        w.write_u32::<BigEndian>(compressed.len() as u32)?;
        w.write_all(&compressed)?;
        return Ok(());
    }

    // Иначе пишем как есть
    w.write_all(&buf)
}

/// Внутренняя сериализация значения без упаковки в сжатый блок
pub fn write_value_inner<W: Write>(
    w: &mut W,
    v: &Value,
) -> std::io::Result<()> {
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
                write_value_inner(w, &Value::Str(item.clone()))?;
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

pub fn write_dump<W: Write>(
    w: &mut W,
    kvs: impl Iterator<Item = (Sds, Value)>,
) -> std::io::Result<()> {
    // Заголовок: ZDB + версия.
    w.write_all(FILE_MAGIC)?;
    w.write_u8(DUMP_VERSION)?;

    // Теперь - число записей (ключей) или сразу данные?
    // Например, пишем число пар <ключ, значение>:
    w.write_u32::<BigEndian>(kvs.size_hint().0 as u32)?;

    // Итерируем пары и сериализуем
    for (key, val) in kvs {
        let kb = key.as_bytes();
        w.write_u32::<BigEndian>(kb.len() as u32)?;
        w.write_all(kb)?;
        write_value(w, &val)?;
    }

    Ok(())
}

/// Пишет в `w` потоковую сериализацию дампа:
/// - магия + версия;
/// - затем N записей <ключ,значение>;
/// - в конце — `TAG_EOF`.
pub fn write_stream<W: Write>(
    w: &mut W,
    kvs: impl Iterator<Item = (Sds, Value)>,
) -> std::io::Result<()> {
    w.write_all(FILE_MAGIC)?;
    w.write_u8(DUMP_VERSION)?;

    for (key, val) in kvs {
        let kb = key.as_bytes();
        w.write_u32::<BigEndian>(kb.len() as u32)?;
        w.write_all(kb)?;
        write_value(w, &val)?;
    }

    w.write_u8(TAG_EOF)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{decompress_block, read_value, StreamReader},
        Sds,
    };

    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_write_read_int() {
        let original = Value::Int(-123456);
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = read_value(&mut cursor).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_write_read_float() {
        let original = Value::Float(std::f64::consts::PI);
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = read_value(&mut cursor).unwrap();

        match decoded {
            Value::Float(f) => assert!((f - std::f64::consts::PI).abs() < 1e-10),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_write_read_bool() {
        let original = Value::Bool(true);
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = read_value(&mut cursor).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_write_read_null() {
        let original = Value::Null;
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = read_value(&mut cursor).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_write_read_str() {
        let original = Value::Str(Sds::from_str("hello"));
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = read_value(&mut cursor).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_should_compress_threshold() {
        assert!(!should_compress(0));
        assert!(!should_compress(63));
        assert!(!should_compress(64 - 1));
        assert!(should_compress(64));
        assert!(should_compress(1000));
    }

    #[test]
    fn test_compress_decompress_roundtrip_small() {
        let data = b"short data";
        // small data: compress_block still works, but write logic won't use it
        let compressed = compress_block(data).expect("compress failed");
        let decompressed = decompress_block(&compressed).expect("decompress failed");
        assert_eq!(&decompressed, data);
    }

    #[test]
    fn test_compress_decompress_roundtrip_large() {
        // generate > MIN_COMPRESSION_SIZE bytes
        let data: Vec<u8> = (0..200).map(|i| (i % 256) as u8).collect();
        assert!(should_compress(data.len()));
        let compressed = compress_block(&data).expect("compress failed");
        // compressed buffer must be smaller or at least non-zero
        assert!(!compressed.is_empty());
        let decompressed = decompress_block(&compressed).expect("decompress failed");
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_decompress_invalid_data() {
        // random bytes should error
        let bad = vec![0u8; 10];
        let err = decompress_block(&bad).unwrap_err();
        // error kind is Other (from zstd)
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn test_stream_roundtrip() {
        let items = vec![
            (Sds::from_str("a"), Value::Int(1)),
            (Sds::from_str("b"), Value::Str(Sds::from_str("c"))),
        ];
        let mut buf = Vec::new();
        write_stream(&mut buf, items.clone().into_iter()).unwrap();

        let reader = StreamReader::new(&buf[..]).unwrap();
        let got: Vec<_> = reader.map(|res| res.unwrap()).collect();
        assert_eq!(got, items);
    }

    #[test]
    fn test_stream_empty() {
        let mut buf = Vec::new();
        write_stream(&mut buf, Vec::<(Sds, Value)>::new().into_iter()).unwrap();

        let mut reader = StreamReader::new(&buf[..]).unwrap();
        assert!(reader.next().is_none());
    }
}
