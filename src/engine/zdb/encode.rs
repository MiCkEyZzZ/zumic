//! Модуль для сериализации значений `Value` в бинарный формат.
//!
//! Все типы значений кодируются с префиксным тегом, за которым следует длина и
//! содержимое (если применимо).
//!
//! Используется BigEndian-формат для чисел.

use std::io::Write;

use byteorder::{BigEndian, WriteBytesExt};
use crc32fast::Hasher;
use zumic_error::{ResultExt, ZdbError, ZumicResult};

use super::{
    compress_block, should_compress, FormatVersion, FILE_MAGIC, TAG_ARRAY, TAG_BITMAP, TAG_BOOL,
    TAG_COMPRESSED, TAG_EOF, TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_LIST, TAG_NULL, TAG_SET,
    TAG_SSTREAM, TAG_STR, TAG_ZSET,
};
use crate::{Sds, Value};

/// Сериализует значение [`Value`] в поток `Write` с автоматическим сжатием.
pub fn write_value<W: Write>(
    w: &mut W,
    v: &Value,
) -> ZumicResult<()> {
    // Сначала сериализуем значение во внутренний буфер
    let mut buf = Vec::new();
    write_value_inner(&mut buf, v)?;

    // Если буфер большой — сжимаем его
    if should_compress(buf.len()) {
        let compressed = compress_block(&buf).map_err(|e| ZdbError::CompressionError {
            operation: zumic_error::CompressionOp::Compress,
            reason: format!("zstd compression failed: {e}"),
            offset: None,
            key: None,
            compressed_size: Some(buf.len() as u32),
        })?;

        w.write_u8(TAG_COMPRESSED)
            .context("Failed to write compressed tag")?;
        w.write_u32::<BigEndian>(compressed.len() as u32)
            .context("Failed to write compressed length")?;
        w.write_all(&compressed)
            .context("Failed to write compressed data")?;
        return Ok(());
    }

    // Иначе пишем как есть
    w.write_all(&buf).context("Failed to write value data")?;
    Ok(())
}

/// Внутренняя сериализация значения без упаковки в сжатый блок
pub fn write_value_inner<W: Write>(
    w: &mut W,
    v: &Value,
) -> ZumicResult<()> {
    match v {
        Value::Str(s) => {
            w.write_u8(TAG_STR).context("Failed to write STR tag")?;
            let b = s.as_bytes();
            w.write_u32::<BigEndian>(b.len() as u32)
                .context("Failed to write string length")?;
            w.write_all(b).context("Failed to write string data")
        }
        Value::Int(i) => {
            w.write_u8(TAG_INT).context("Failed to write INT tag")?;
            w.write_i64::<BigEndian>(*i)
                .context("Failed to write int value")
        }
        Value::Float(f) => {
            w.write_u8(TAG_FLOAT).context("Failed to write FLOAT tag")?;
            w.write_f64::<BigEndian>(*f)
                .context("Failed to write float value")
        }
        Value::Bool(b) => {
            w.write_u8(TAG_BOOL).context("Failed to write BOOL tag")?;
            w.write_u8(if *b { 1 } else { 0 })
                .context("Failed to write bool value")
        }
        Value::Null => w.write_u8(TAG_NULL).context("Failed to write NULL tag"),
        Value::List(list) => {
            w.write_u8(TAG_LIST).context("Failed to write LIST tag")?;
            w.write_u32::<BigEndian>(list.len() as u32)
                .context("Failed to write list length")?;
            for item in list.iter() {
                write_value_inner(w, &Value::Str(item.clone()))?;
            }
            Ok(())
        }
        Value::Array(arr) => {
            w.write_u8(TAG_ARRAY).context("Failed to write ARRAY tag")?;
            w.write_u32::<BigEndian>(arr.len() as u32)
                .context("Failed to write array length")?;
            for item in arr {
                write_value_inner(w, item)?;
            }
            Ok(())
        }
        Value::Hash(hmap) => {
            w.write_u8(TAG_HASH).context("Failed to write HASH tag")?;
            w.write_u32::<BigEndian>(hmap.len() as u32)
                .context("Failed to write hash size")?;
            // entries() возвращает Vec<(Sds, Sds)>, не требует &mut
            for (field, val) in hmap.entries() {
                // ключ
                let fb = field.as_bytes();
                w.write_u32::<BigEndian>(fb.len() as u32)
                    .context("Failed to write hash key length")?;
                w.write_all(fb).context("Failed to write hash key")?;
                // значение — строка (Sds)
                w.write_u8(TAG_STR)
                    .context("Failed to write hash value STR tag")?;
                let vb = val.as_bytes();
                w.write_u32::<BigEndian>(vb.len() as u32)
                    .context("Failed to write hash value length")?;
                w.write_all(vb).context("Failed to write hash value data")?;
            }
            Ok(())
        }
        Value::ZSet { dict, sorted } => {
            w.write_u8(TAG_ZSET).context("Failed to write ZSET tag")?;
            w.write_u32::<BigEndian>(dict.len() as u32)
                .context("Failed to write zset size")?;
            for (score_wrapper, key) in sorted.iter() {
                let score = score_wrapper.into_inner();
                let kb = key.as_bytes();
                w.write_u32::<BigEndian>(kb.len() as u32)
                    .context("Failed to write zset key length")?;
                w.write_all(kb).context("Failed to write zset key")?;
                w.write_f64::<BigEndian>(score)
                    .context("Failed to write zset score")?;
            }
            Ok(())
        }
        Value::Set(s) => {
            w.write_u8(TAG_SET).context("Failed to write SET tag")?;
            w.write_u32::<BigEndian>(s.len() as u32)
                .context("Failed to write set size")?;
            for member in s.iter() {
                let mb = member.as_bytes();
                w.write_u32::<BigEndian>(mb.len() as u32)
                    .context("Failed to write set member length")?;
                w.write_all(mb).context("Failed to write set member")?;
            }
            Ok(())
        }
        Value::HyperLogLog(hll) => {
            w.write_u8(TAG_HLL).context("Failed to write HLL tag")?;
            w.write_u32::<BigEndian>(hll.data.len() as u32)
                .context("Failed to write HLL size")?;
            w.write_all(&hll.data).context("Failed to write HLL data")?;
            Ok(())
        }
        Value::SStream(entries) => {
            w.write_u8(TAG_SSTREAM)
                .context("Failed to write SSTREAM tag")?;
            w.write_u32::<BigEndian>(entries.len() as u32)
                .context("Failed to write stream size")?;
            for entry in entries {
                w.write_u64::<BigEndian>(entry.id.ms_time)
                    .context("Failed to write stream entry ms_time")?;
                w.write_u64::<BigEndian>(entry.id.sequence)
                    .context("Failed to write stream entry sequence")?;
                w.write_u32::<BigEndian>(entry.data.len() as u32)
                    .context("Failed to write stream fields count")?;
                for (field, val) in entry.data.iter() {
                    let fb = field.as_bytes();
                    w.write_u32::<BigEndian>(fb.len() as u32)
                        .context("Failed to write stream field length")?;
                    w.write_all(fb).context("Failed to write stream field")?;
                    write_value(w, val)?;
                }
            }
            Ok(())
        }
        Value::Bitmap(bm) => {
            w.write_u8(TAG_BITMAP)
                .context("Failed to write BITMAP tag")?;
            let bytes = bm.as_bytes();
            w.write_u32::<BigEndian>(bytes.len() as u32)
                .context("Failed to write bitmap size")?;
            w.write_all(bytes).context("Failed to write bitmap data")?;
            Ok(())
        }
    }
}

/// Записывает дамп с проверкой целостности: магия, версия, записи и CRC32 в
/// конце.
///
/// Формат:
///   [magic][ver][count]
///   ... пары <key, value> ...
///   [crc32: u32 BE]
pub fn write_dump<W: Write>(
    w: &mut W,
    kvs: impl Iterator<Item = (Sds, Value)>,
) -> ZumicResult<()> {
    // 1) Собираем «тело» дампа в буфер
    let mut buf = Vec::new();
    buf.extend_from_slice(FILE_MAGIC);
    buf.push(FormatVersion::V1 as u8);

    let items: Vec<_> = kvs.collect();
    buf.write_u32::<BigEndian>(items.len() as u32)
        .context("Failed to write item count")?;

    for (key, val) in items {
        let kb = key.as_bytes();
        buf.write_u32::<BigEndian>(kb.len() as u32)
            .context("Failed to write key length")?;
        buf.write_all(kb).context("Failed to write key")?;
        write_value(&mut buf, &val)?;
    }

    // 2) Вычисляем CRC32 от всего буфера
    let mut hasher = Hasher::new();
    hasher.update(&buf);
    let crc = hasher.finalize();

    // 3) Пишем буфер и CRC32
    w.write_all(&buf).context("Failed to write dump body")?;
    w.write_u32::<BigEndian>(crc)
        .context("Failed to write CRC32")?;
    Ok(())
}

/// Пишет в `w` потоковую сериализацию дампа:
/// - магия + версия;
/// - затем N записей <ключ,значение>;
/// - в конце — `TAG_EOF`.
pub fn write_stream<W: Write>(
    w: &mut W,
    kvs: impl Iterator<Item = (Sds, Value)>,
) -> ZumicResult<()> {
    w.write_all(FILE_MAGIC).context("Failed to write magic")?;
    w.write_u8(FormatVersion::V1 as u8)
        .context("Failed to write version")?;

    for (key, val) in kvs {
        let kb = key.as_bytes();
        w.write_u32::<BigEndian>(kb.len() as u32)
            .context("Failed to write key length")?;
        w.write_all(kb).context("Failed to write key")?;
        write_value(w, &val)?;
    }

    w.write_u8(TAG_EOF).context("Failed to write EOF tag")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::{
        database::Bitmap,
        engine::{decompress_block, read_dump, read_value_with_version, StreamReader},
        Sds,
    };

    /// Тест проверяет сериализацию и десериализацию целого числа.
    #[test]
    fn test_write_read_int() {
        let original = Value::Int(-123456);
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap();
        assert_eq!(decoded, original);
    }

    /// Тест проверяет сериализацию и десериализацию числа с плавающей точкой.
    #[test]
    fn test_write_read_float() {
        let original = Value::Float(std::f64::consts::PI);
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap();

        match decoded {
            Value::Float(f) => assert!((f - std::f64::consts::PI).abs() < 1e-10),
            _ => panic!("Expected Float"),
        }
    }

    /// Тест проверяет сериализацию и десериализацию булевого значения.
    #[test]
    fn test_write_read_bool() {
        let original = Value::Bool(true);
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap();

        assert_eq!(decoded, original);
    }

    /// Тест проверяет сериализацию и десериализацию null-значения.
    #[test]
    fn test_write_read_null() {
        let original = Value::Null;
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap();

        assert_eq!(decoded, original);
    }

    /// Тест проверяет сериализацию и десериализацию строки.
    #[test]
    fn test_write_read_str() {
        let original = Value::Str(Sds::from_str("hello"));
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap();

        assert_eq!(decoded, original);
    }

    /// Тест проверяет поведение `should_compress` при различных размерах
    /// входных данных.
    #[test]
    fn test_should_compress_threshold() {
        assert!(!should_compress(0));
        assert!(!should_compress(63));
        assert!(!should_compress(64 - 1));
        assert!(should_compress(64));
        assert!(should_compress(1000));
    }

    /// Тест проверяет, что маленькие данные можно сжать и успешно распаковать.
    #[test]
    fn test_compress_decompress_roundtrip_small() {
        let data = b"short data";
        // small data: compress_block still works, but write logic won't use it
        let compressed = compress_block(data).expect("compress failed");
        let decompressed = decompress_block(&compressed).expect("decompress failed");
        assert_eq!(&decompressed, data);
    }

    /// Тест проверяет, что большие данные корректно сжимаются и
    /// распаковываются.
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

    /// Тест проверяет обработку ошибки при попытке распаковать случайные
    /// данные.
    #[test]
    fn test_decompress_invalid_data() {
        // random bytes should error
        let bad = vec![0u8; 10];
        let err = decompress_block(&bad).unwrap_err();
        // error kind is Other (from zstd)
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
    }

    /// Тест проверяет корректность записи и чтения потока данных (stream
    /// roundtrip).
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

    /// Тест проверяет сериализацию и десериализацию пустого потока.
    #[test]
    fn test_stream_empty() {
        let mut buf = Vec::new();
        write_stream(&mut buf, Vec::<(Sds, Value)>::new().into_iter()).unwrap();

        let mut reader = StreamReader::new(&buf[..]).unwrap();
        assert!(reader.next().is_none());
    }

    /// Тест проверяет, что дамп с CRC проходит полный круг: write_dump →
    /// read_dump.
    #[test]
    fn doc_test_dump_roundtrip_crc() {
        let items = vec![
            (Sds::from_str("foo"), Value::Int(123)),
            (Sds::from_str("bar"), Value::Str(Sds::from_str("baz"))),
        ];
        let mut buf = Vec::new();
        write_dump(&mut buf, items.clone().into_iter()).unwrap();
        let got = read_dump(&mut &buf[..]).unwrap();
        assert_eq!(got, items);
    }

    /// Тест проверяет, что при повреждении CRC в конце read_dump падает с
    /// ошибкой.
    #[test]
    fn doc_test_dump_crc_mismatch() {
        let items = vec![(Sds::from_str("key"), Value::Bool(false))];
        let mut buf = Vec::new();
        write_dump(&mut buf, items.into_iter()).unwrap();

        // «Поломаем» последний (CRC) байт
        let last = buf.len() - 1;
        buf[last] ^= 0xFF;

        let err = read_dump(&mut &buf[..]).unwrap_err();

        // Проверяем что это ошибка CRC - тип ошибки может варьироваться
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("CRC") || err_msg.contains("mismatch") || err_msg.contains("checksum"),
            "Expected CRC-related error, got: {err_msg}"
        );
    }

    /// Тест проверяет запись и чтение пустого дампа.
    #[test]
    fn doc_test_dump_empty() {
        let mut buf = Vec::new();
        write_dump(&mut buf, Vec::<(Sds, Value)>::new().into_iter()).unwrap();
        let got = read_dump(&mut &buf[..]).unwrap();
        assert!(got.is_empty());
    }

    /// Тест проверяет ошибку при чтении дампа с некорректной магией.
    #[test]
    fn doc_test_dump_bad_magic() {
        let mut buf = Vec::new();
        buf.extend(b"BAD"); // неправильная магия
        buf.push(FormatVersion::V1 as u8);
        buf.extend(&0u32.to_be_bytes()); // count = 0
        buf.extend(&0u32.to_be_bytes()); // фиктивный CRC32

        assert!(read_dump(&mut &buf[..]).is_err());
    }

    /// Проверяет сериализацию и десериализацию массива через TAG_ARRAY.
    #[test]
    fn test_write_read_array() {
        let original = Value::Array(vec![Value::Int(42), Value::Str(Sds::from_str("foo"))]);
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap();
        assert_eq!(decoded, original);
    }

    /// Проверяет сериализацию и десериализацию битмапы через TAG_BITMAP.
    #[test]
    fn test_write_read_bitmap() {
        let mut bm = Bitmap::new();
        bm.bytes = vec![0xAA, 0xBB, 0xCC];
        let original = Value::Bitmap(bm.clone());
        let mut buf = Vec::new();
        write_value(&mut buf, &original).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap();
        if let Value::Bitmap(decoded_bm) = decoded {
            assert_eq!(decoded_bm.as_bytes(), &bm.bytes[..]);
        } else {
            panic!("Expected Bitmap");
        }
    }

    /// Проверяет, что потоковая сериализация останавливается на TAG_EOF.
    #[test]
    fn test_write_stream_eof() {
        let mut buf = Vec::new();
        buf.extend(FILE_MAGIC);
        buf.push(FormatVersion::V1 as u8);
        buf.push(TAG_EOF);

        let mut reader = StreamReader::new(&buf[..]).unwrap();
        assert!(reader.next().is_none());
    }
}
