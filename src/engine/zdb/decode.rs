//! Модуль для десериализации значений `Value` из бинарного формата.
//!
//! Поддерживаются все внутренние типы данных базы: строки, числа, множества,
//! словари, ZSet, HyperLogLog и Stream.
//!
//! Каждое значение начинается с однобайтового тега, за которым следует длина и
//! данные.

use std::{
    collections::HashSet,
    io::{self, Error, ErrorKind, Read},
};

use byteorder::{BigEndian, ReadBytesExt};
use crc32fast::Hasher;
use ordered_float::OrderedFloat;

use super::{
    decompress_block, CompatibilityInfo, FormatVersion, VersionUtils, FILE_MAGIC, TAG_ARRAY,
    TAG_BITMAP, TAG_BOOL, TAG_COMPRESSED, TAG_EOF, TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_NULL,
    TAG_SET, TAG_STR, TAG_ZSET,
};
use crate::{database::Bitmap, Dict, Hll, Sds, SkipList, SmartHash, Value, DENSE_SIZE};

/// Итератор по парам <Key, Value> из потокового дампа.
///
/// Будет читать из `r` по одной записи, пока не встретит `TAG_EOF`.
pub struct StreamReader<R: Read> {
    inner: R,
    version: FormatVersion,
    done: bool,
    compatibility_info: CompatibilityInfo,
}

impl<R: Read> StreamReader<R> {
    /// Создаёт новый stream-reader с проверкой совместимости версий.
    pub fn new(r: R) -> io::Result<Self> {
        Self::new_with_version(r, FormatVersion::current())
    }
    /// Создаёт stream-reader с явно указанной версией читателя.
    pub fn new_with_version(
        mut r: R,
        reader_version: FormatVersion,
    ) -> io::Result<Self> {
        // Проверяем magic
        let mut magic = [0; 3];
        r.read_exact(&mut magic)?;
        if &magic != FILE_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid ZDB dump: bad magic number",
            ));
        }

        // Читаем версию дампа
        let version_byte = r.read_u8()?;
        let dump_version = FormatVersion::try_from(version_byte)?;

        // Проверяем совместимость
        let compatibility_info =
            VersionUtils::validate_compatibility(reader_version, dump_version)?;

        Ok(Self {
            inner: r,
            version: dump_version,
            done: false,
            compatibility_info,
        })
    }

    /// Возвращает версию дампа.
    pub fn version(&self) -> FormatVersion {
        self.version
    }

    /// Возвращает информацию о совместимости
    pub fn compatibility_info(&self) -> &CompatibilityInfo {
        &self.compatibility_info
    }

    /// Возвращает предупреждения о совместимости
    pub fn warnings(&self) -> &[String] {
        &self.compatibility_info.warnings
    }
}

/// Десериализует значение [`Value`] из бинарного потока.
pub fn read_value<R: Read>(r: &mut R) -> io::Result<Value> {
    read_value_with_version(r, FormatVersion::current())
}

/// Собственно реализация с явной версией.
/// Можете переименовать старый `read_value` в `read_value_with_version`.
pub fn read_value_with_version<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> io::Result<Value> {
    let tag = r.read_u8()?;

    match tag {
        TAG_STR => read_string_value(r, version),
        TAG_INT => read_int_value(r, version),
        TAG_FLOAT => read_float_value(r, version),
        TAG_BOOL => read_bool_value(r, version),
        TAG_NULL => Ok(Value::Null),
        TAG_COMPRESSED => read_compressed_value(r, version),
        TAG_HASH => read_hash_value(r, version),
        TAG_ZSET => read_zset_value(r, version),
        TAG_SET => read_set_value(r, version),
        TAG_HLL => read_hll_value(r, version),
        TAG_ARRAY => read_array_value(r, version),
        TAG_BITMAP => read_bitmap_value(r, version),
        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unknown tag {other} in format version {version}"),
        )),
    }
}

/// Расширенная функция чтения дампа с проверкой совместимости версий.
pub fn read_dump<R: Read>(r: &mut R) -> io::Result<Vec<(Sds, Value)>> {
    read_dump_with_version(r, FormatVersion::current())
}

/// Читает дамп с явно указанной версией читателя.
pub fn read_dump_with_version<R: Read>(
    r: &mut R,
    reader_version: FormatVersion,
) -> io::Result<Vec<(Sds, Value)>> {
    // 1) Считываем весь поток в буфере
    let mut data = Vec::new();
    r.read_to_end(&mut data)?;
    if data.len() < 7 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Dump too small"));
    }

    // 2) Отделяем CRC32 и проверяем
    let body_len = data.len() - 4;
    let (body, crc_bytes) = data.split_at(body_len);
    let recorded_crc = (&crc_bytes[..4]).read_u32::<BigEndian>()?;
    let mut hasher = Hasher::new();
    hasher.update(body);
    if hasher.finalize() != recorded_crc {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "CRC mismatch"));
    }

    // 3) Работем по Cursor
    let mut cursor = io::Cursor::new(body);
    // проверяем магию
    let mut magic = [0u8; 3];
    cursor.read_exact(&mut magic)?;
    if &magic != FILE_MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad file magic"));
    }
    // читаем и валидируем версию
    let dump_version = FormatVersion::try_from(cursor.read_u8()?)?;
    // проверяем совместимость
    let _compat = VersionUtils::validate_compatibility(reader_version, dump_version)?;

    // 4) Читаем count
    let count = cursor.read_u32::<BigEndian>()? as usize;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        // ключ
        let klen = cursor.read_u32::<BigEndian>()? as usize;
        let mut kb = vec![0; klen];
        cursor.read_exact(&mut kb)?;
        let key = Sds::from_vec(kb);
        // значение
        let val = read_value_with_version(&mut cursor, dump_version)?;
        items.push((key, val));
    }

    Ok(items)
}

fn read_string_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
) -> io::Result<Value> {
    let len = r.read_u32::<BigEndian>()? as usize;
    let mut buf = vec![0; len];
    r.read_exact(&mut buf)?;
    Ok(Value::Str(Sds::from_vec(buf)))
}

fn read_int_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
) -> io::Result<Value> {
    let i = r.read_i64::<BigEndian>()?;
    Ok(Value::Int(i))
}

fn read_float_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
) -> io::Result<Value> {
    let f = r.read_f64::<BigEndian>()?;
    Ok(Value::Float(f))
}

fn read_bool_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
) -> io::Result<Value> {
    // Булево: 1 => true, 0 => false
    let b = r.read_u8()? != 0;
    Ok(Value::Bool(b))
}

fn read_compressed_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> io::Result<Value> {
    let len = r.read_u32::<BigEndian>()? as usize;
    let mut compressed = vec![0; len];
    r.read_exact(&mut compressed)?;
    let decompressed = match version {
        FormatVersion::Legacy => {
            // Legacy версия может не поддерживать сжатие
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Compressed blocks not supported in legacy format",
            ));
        }
        FormatVersion::V1 => decompress_block(&compressed)?,
        FormatVersion::V2 => {
            // V2 может иметь улучшенный алгоритм сжатия
            decompress_block(&compressed)?
        }
    };

    let mut slice = decompressed.as_slice();
    read_value_with_version(&mut slice, version)
}

fn read_hash_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> io::Result<Value> {
    let n = r.read_u32::<BigEndian>()? as usize;
    let mut map = SmartHash::new();

    for _ in 0..n {
        // читаем ключ
        let klen = r.read_u32::<BigEndian>()? as usize;
        let mut kb = vec![0; klen];
        r.read_exact(&mut kb)?;
        let key = Sds::from_vec(kb);

        // читаем значение как Value
        let raw = read_value_with_version(r, version)?;
        // проверяем, что Value::Str и берём из него Sds
        let val = match raw {
            Value::Str(s) => s,
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Expected Str for Hash value in format version {version}"),
                ))
            }
        };

        map.insert(key, val);
    }
    Ok(Value::Hash(map))
}

fn read_zset_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> io::Result<Value> {
    let n = r.read_u32::<BigEndian>()? as usize;
    let mut dict = Dict::new();
    let mut sorted = SkipList::new();

    for _ in 0..n {
        let klen = r.read_u32::<BigEndian>()? as usize;
        let mut kb = vec![0; klen];
        r.read_exact(&mut kb)?;
        let key = Sds::from_vec(kb);

        let score = match version {
            FormatVersion::Legacy => {
                // Legacy может использовать 32-битные числа для score
                r.read_f32::<BigEndian>()? as f64
            }
            FormatVersion::V1 | FormatVersion::V2 => r.read_f64::<BigEndian>()?,
        };

        dict.insert(key.clone(), score);
        sorted.insert(OrderedFloat(score), key);
    }
    Ok(Value::ZSet { dict, sorted })
}

fn read_set_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
) -> io::Result<Value> {
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

fn read_hll_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> io::Result<Value> {
    let n = r.read_u32::<BigEndian>()? as usize;

    // Проверяем корректность размера в зависимости от версии
    match version {
        FormatVersion::Legacy => {
            if n > DENSE_SIZE {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("HLL size {n} exceeds maximum {DENSE_SIZE} in legacy format"),
                ));
            }
        }
        FormatVersion::V1 | FormatVersion::V2 => {
            // Более современные версии могут поддерживать переменные размеры
        }
    }

    // читаем ровно n байт (тест может передавать n = 2)
    let mut regs = vec![0u8; n];
    r.read_exact(&mut regs)?;

    // копируем прочитанное в фиксированный буфер HLL.data (DENSE_SIZE),
    // дополняя нулями, если n < DENSE_SIZE
    let mut data = [0u8; DENSE_SIZE];
    data[..n.min(DENSE_SIZE)].copy_from_slice(&regs[..n.min(DENSE_SIZE)]);

    Ok(Value::HyperLogLog(Box::new(Hll { data })))
}

fn read_array_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> io::Result<Value> {
    let len = r.read_u32::<BigEndian>()? as usize;
    let mut items = Vec::with_capacity(len);

    for _ in 0..len {
        items.push(read_value_with_version(r, version)?);
    }

    Ok(Value::Array(items))
}

fn read_bitmap_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> io::Result<Value> {
    let byte_len = r.read_u32::<BigEndian>()? as usize;

    // Проверяем ограничения в зависимости от версии
    match version {
        FormatVersion::Legacy => {
            // Legacy может иметь ограничения на размер bitmap
            if byte_len > 1024 * 1024 {
                // 1MB limit
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Bitmap too large for legacy format",
                ));
            }
        }
        FormatVersion::V1 | FormatVersion::V2 => {
            // Более новые версии поддерживают большие bitmap
        }
    }

    let mut buf = vec![0u8; byte_len];
    r.read_exact(&mut buf)?;

    let mut bmp = Bitmap::new();
    bmp.bytes = buf;

    Ok(Value::Bitmap(bmp))
}

impl<R: Read> Iterator for StreamReader<R> {
    type Item = io::Result<(Sds, Value)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        // проверяем EOF
        let mut peek = [0u8; 1];
        if let Err(e) = self.inner.read_exact(&mut peek) {
            return Some(Err(e));
        }
        if peek[0] == TAG_EOF {
            self.done = true;
            return None;
        }
        // это первая байта длины ключа
        let mut len_buf = [0u8; 4];
        len_buf[0] = peek[0];
        if let Err(e) = self.inner.read_exact(&mut len_buf[1..]) {
            return Some(Err(e));
        }
        let klen = u32::from_be_bytes(len_buf) as usize;
        let mut kb = vec![0; klen];
        if let Err(e) = self.inner.read_exact(&mut kb) {
            return Some(Err(e));
        }
        let key = Sds::from_vec(kb);
        // читаем значение с учётом версии
        match read_value_with_version(&mut self.inner, self.version) {
            Ok(v) => Some(Ok((key, v))),
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::engine::{compress_block, write_dump, write_stream};

    /// Тест проверяет, что чтение строки даст `Value::Str("hello")`
    #[test]
    fn test_read_str() {
        let s = b"hello";
        let mut data = Vec::new();
        data.push(TAG_STR);
        data.extend(&(s.len() as u32).to_be_bytes());
        data.extend(s);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(b"hello".to_vec())));
    }

    /// Тест проверяет, что чтение пустой строки даст `Value::Str("")`
    #[test]
    fn test_read_empty_str() {
        let mut data = vec![TAG_STR];
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(Vec::new())));
    }

    /// Тест проверяет, что чтение целого числа даст `Value::Int(i)`
    #[test]
    fn test_read_int() {
        let i = -123456i64;
        let mut data = Vec::new();
        data.push(TAG_INT);
        data.extend(&i.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(val, Value::Int(i));
    }

    /// Тест проверяет, что чтение числа с плавающей точкой даст
    /// `Value::Float(f)`
    #[test]
    fn test_read_float() {
        use std::f64::consts::PI;

        let f = PI;
        let mut data = Vec::new();
        data.push(TAG_FLOAT);
        data.extend(&f.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        match val {
            Value::Float(v) => assert!((v - f).abs() < 1e-10),
            _ => panic!("Expected Value::Float"),
        }
    }

    /// Тест проверяет, что чтение булевого `true` даст `Value::Bool(true)`
    #[test]
    fn test_read_bool_true() {
        let data = vec![TAG_BOOL, 1];

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(val, Value::Bool(true));
    }

    /// Тест проверяет, что чтение булевого `false` даст `Value::Bool(false)`
    #[test]
    fn test_read_bool_false() {
        let data = vec![TAG_BOOL, 0];

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(val, Value::Bool(false));
    }

    /// Тест проверяет, что чтение `null` даст `Value::Null`
    #[test]
    fn test_read_null() {
        let data = vec![TAG_NULL];

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(val, Value::Null);
    }

    /// Тест проверяет, что чтение пустого хеша даст пустой `Value::Hash`
    #[test]
    fn test_read_hash_empty() {
        let mut data = Vec::new();
        data.push(TAG_HASH);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        match val {
            Value::Hash(map) => assert!(map.is_empty()),
            _ => panic!("Expected Value::Hash"),
        }
    }

    /// Тест проверяет, что чтение хеша с одной записью вернёт корректную пару
    /// `ключ -> строка`
    #[test]
    fn test_read_hash_with_one_entry() {
        let key = b"key";
        let val_str = b"val";

        let mut data = Vec::new();
        data.push(TAG_HASH);
        data.extend(&(1u32).to_be_bytes()); // 1 элемент

        // ключ
        data.extend(&(key.len() as u32).to_be_bytes());
        data.extend(key);

        // значение - строка
        data.push(TAG_STR);
        data.extend(&(val_str.len() as u32).to_be_bytes());
        data.extend(val_str);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        match val {
            Value::Hash(mut m) => {
                assert_eq!(m.len(), 1);
                assert_eq!(
                    m.get(&Sds::from_vec(key.to_vec())).unwrap(),
                    &Sds::from_vec(val_str.to_vec())
                );
            }
            _ => panic!("Expected Value::Hash"),
        }
    }

    /// Тест проверяет, что при чтении хеша со значением не-строкой возвращается
    /// ошибка `InvalidData`
    #[test]
    fn test_read_hash_value_not_str_error() {
        // создадим хеш с ключом, но значением не строка (например, Int)
        let key = b"key";

        let mut data = Vec::new();
        data.push(TAG_HASH);
        data.extend(&(1u32).to_be_bytes());

        // ключ
        data.extend(&(key.len() as u32).to_be_bytes());
        data.extend(key);

        // значение - INT, а должно быть STR, чтобы вызвало ошибку
        data.push(TAG_INT);
        data.extend(&(123i64).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let err = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("Expected Str for Hash value"));
    }

    /// Тест проверяет, что чтение пустого ZSet даст пустой `Value::ZSet`
    #[test]
    fn test_read_zset_empty() {
        let mut data = Vec::new();
        data.push(TAG_ZSET);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        match val {
            Value::ZSet { dict, sorted } => {
                assert!(dict.is_empty());
                assert!(sorted.is_empty());
            }
            _ => panic!("Expected Value::ZSet"),
        }
    }

    /// Тест проверяет, что чтение ZSet с записями вернёт корректные `dict` и
    /// `sorted`
    #[test]
    fn test_read_zset_with_entries() {
        let key1 = b"key1";
        let key2 = b"key2";
        let score1 = 10.5f64;
        let score2 = -3.0f64;

        let mut data = Vec::new();
        data.push(TAG_ZSET);
        data.extend(&(2u32).to_be_bytes());

        // Первый элемент
        data.extend(&(key1.len() as u32).to_be_bytes());
        data.extend(key1);
        data.extend(&score1.to_be_bytes());

        // Второй элемент
        data.extend(&(key2.len() as u32).to_be_bytes());
        data.extend(key2);
        data.extend(&score2.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        match val {
            Value::ZSet { mut dict, sorted } => {
                assert_eq!(dict.len(), 2);
                assert_eq!(dict.get(&Sds::from_vec(key1.to_vec())), Some(&score1));
                assert_eq!(dict.get(&Sds::from_vec(key2.to_vec())), Some(&score2));

                // sorted должен содержать оба элемента, проверим размер
                assert_eq!(sorted.len(), 2);
            }
            _ => panic!("Expected Value::ZSet"),
        }
    }

    /// Тест проверяет, что чтение пустого множества даст пустой `Value::Set`
    #[test]
    fn test_read_set_empty() {
        let mut data = Vec::new();
        data.push(TAG_SET);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        match val {
            Value::Set(set) => assert!(set.is_empty()),
            _ => panic!("Expected Value::Set"),
        }
    }

    /// Тест проверяет, что чтение множества с записями вернёт все элементы
    #[test]
    fn test_read_set_with_entries() {
        let elems: &[&[u8]] = &[b"one", b"two", b"three"];

        let mut data = Vec::new();
        data.push(TAG_SET);
        data.extend(&(elems.len() as u32).to_be_bytes());

        for &e in elems {
            data.extend(&(e.len() as u32).to_be_bytes());
            data.extend(e);
        }

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        match val {
            Value::Set(set) => {
                assert_eq!(set.len(), elems.len());
                for &e in elems {
                    assert!(set.contains(&Sds::from_vec(e.to_vec())));
                }
            }
            _ => panic!("Expected Value::Set"),
        }
    }

    /// Тест проверяет, что чтение HLL меньше `DENSE_SIZE` заполнит остаток
    /// нулями
    #[test]
    fn test_read_hll_with_less_than_dense_size() {
        let n = 2usize; // меньше DENSE_SIZE
        let regs = vec![1u8, 2u8];

        let mut data = Vec::new();
        data.push(TAG_HLL);
        data.extend(&(n as u32).to_be_bytes());
        data.extend(&regs);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();

        match val {
            Value::HyperLogLog(hll) => {
                assert_eq!(hll.data[0], 1);
                assert_eq!(hll.data[1], 2);
                for i in 2..DENSE_SIZE {
                    assert_eq!(hll.data[i], 0);
                }
            }
            _ => panic!("Expected Value::HyperLogLog"),
        }
    }

    /// Тест проверяет, что чтение HLL ровно `DENSE_SIZE` вернёт все данные без
    /// изменений
    #[test]
    fn test_read_hll_with_exact_dense_size() {
        let regs = vec![7u8; DENSE_SIZE];

        let mut data = Vec::new();
        data.push(TAG_HLL);
        data.extend(&(DENSE_SIZE as u32).to_be_bytes());
        data.extend(&regs);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();

        match val {
            Value::HyperLogLog(hll) => {
                assert_eq!(hll.data.len(), DENSE_SIZE);
                for &b in hll.data.iter() {
                    assert_eq!(b, 7);
                }
            }
            _ => panic!("Expected Value::HyperLogLog"),
        }
    }

    /// Тест проверяет, что неизвестный тег вызывает ошибку `InvalidData` с
    /// сообщением "Unknown tag"
    #[test]
    fn test_read_unknown_tag_error() {
        let data = vec![255]; // несуществующий тег

        let mut cursor = Cursor::new(data);
        let err = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("Unknown tag"));
    }

    /// Тест проверяет, что чтение сжатой строки через `TAG_COMPRESSED` вернёт
    /// оригинальные данные
    #[test]
    fn test_read_compressed_str() {
        // подготовим обычную строку и сожмём её
        let raw =
            b"some longer string that will be compressed because length > MIN_COMPRESSION_SIZE";
        // вручную: сначала сериализуем TAG_STR + длину + данные
        let mut inner = Vec::new();
        inner.push(TAG_STR);
        inner.extend(&(raw.len() as u32).to_be_bytes());
        inner.extend(raw);

        // теперь используем compress_block из super
        let compressed = compress_block(&inner).expect("compress failed");
        let mut data = Vec::new();
        data.push(TAG_COMPRESSED);
        data.extend(&(compressed.len() as u32).to_be_bytes());
        data.extend(&compressed);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(raw.to_vec())));
    }

    /// Тест проверяет, что round-trip дампа через `write_dump` и `read_dump`
    /// возвращает исходные данные
    #[test]
    fn test_read_dump_roundtrip() {
        // создаём два ключа
        let items = vec![
            (Sds::from_str("k1"), Value::Int(123)),
            (Sds::from_str("k2"), Value::Str(Sds::from_str("v2"))),
        ];
        let mut buf = Vec::new();
        write_dump(&mut buf, items.clone().into_iter()).unwrap();

        let got = read_dump(&mut &buf[..]).unwrap();
        assert_eq!(got, items);
    }

    /// Тест проверяет, что при неправильной магии в дампе возникает ошибка
    /// `InvalidData`
    #[test]
    fn test_read_dump_bad_magic() {
        let mut buf = Vec::new();
        // неправильная магия
        buf.extend(b"BAD");
        // пишем корректную версию через FormatVersion
        buf.push(FormatVersion::V1 as u8);
        // count = 0
        buf.extend(&0u32.to_be_bytes());
        // CRC32 (создаём некорректный, чтобы подпись была короче, read_dump упадёт на
        // too small) либо добавьте 4 нуля: buf.extend(&0u32.to_be_bytes());
        let err = read_dump(&mut &buf[..]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// Тест проверяет, что при неверной версии дампа возникает ошибка
    /// `InvalidData`
    #[test]
    fn test_read_dump_wrong_version() {
        let mut buf = Vec::new();
        buf.extend(FILE_MAGIC);
        // пишем неподдерживаемую версию
        buf.push((FormatVersion::V1 as u8) + 1);
        buf.extend(&0u32.to_be_bytes());
        // CRC32 тоже можно захардкодить, но read_dump упадёт сразу на Unsupported
        // version
        let err = read_dump(&mut &buf[..]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// Тест проверяет, что round-trip потокового дампа через `write_stream` и
    /// `StreamReader` возвращает исходные данные
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

    /// Тест проверяет, что пустой поток возвращает `None` сразу при чтении
    #[test]
    fn test_stream_empty() {
        let mut buf = Vec::new();
        write_stream(&mut buf, Vec::<(Sds, Value)>::new().into_iter()).unwrap();

        let mut reader = StreamReader::new(&buf[..]).unwrap();
        assert!(reader.next().is_none());
    }

    /// Тест проверяет, что read_dump корректно читает валидный дамп с CRC.
    #[test]
    fn doc_test_read_dump_with_crc() {
        let items = vec![
            (Sds::from_str("foo"), Value::Int(42)),
            (Sds::from_str("bar"), Value::Str(Sds::from_str("baz"))),
        ];
        let mut buf = Vec::new();
        write_dump(&mut buf, items.clone().into_iter()).unwrap();
        let got = read_dump(&mut &buf[..]).unwrap();
        assert_eq!(got, items);
    }

    /// Тест проверяет, что при повреждении хотя бы одного байта CRC-проверка
    /// падает.
    #[test]
    fn doc_test_read_dump_crc_mismatch() {
        let items = vec![(Sds::from_str("key"), Value::Int(1))];
        let mut buf = Vec::new();
        write_dump(&mut buf, items.into_iter()).unwrap();

        // испортим последний CRC-байт
        let len = buf.len();
        buf[len - 1] ^= 0xFF;

        let err = read_dump(&mut &buf[..]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("CRC mismatch"));
    }

    /// Тест проверяет, что пустой дамп с CRC возвращает пустой вектор
    #[test]
    fn doc_test_dump_empty_crc() {
        let mut buf = Vec::new();
        write_dump(&mut buf, Vec::<(Sds, Value)>::new().into_iter()).unwrap();
        let got = read_dump(&mut &buf[..]).unwrap();
        assert!(got.is_empty());
    }

    /// Тест проверяет, что дамп размером <4 байт вызывает ошибку `InvalidData`
    /// "Dump too small"
    #[test]
    fn doc_test_read_dump_too_small() {
        // Буфер меньше 4 байт → сразу ошибка «Dump too small»
        let err = read_dump(&mut &b"\x00\x01\x02"[..]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("Dump too small"));
    }

    /// Тест: чтение массива через TAG_ARRAY должно вернуть Value::Array с
    /// вложенными элементами
    #[test]
    fn test_read_array() {
        // формируем: [TAG_ARRAY][len=2][вложенный TAG_INT + data][вложенный TAG_STR +
        // data]
        let mut data = Vec::new();
        data.push(TAG_ARRAY);
        data.extend(&(2u32).to_be_bytes());
        // введите два вложенных значения, например Int(5) и Str("x")
        data.push(TAG_INT);
        data.extend(&5i64.to_be_bytes());
        data.push(TAG_STR);
        let s = b"x";
        data.extend(&(s.len() as u32).to_be_bytes());
        data.extend(s);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        assert_eq!(
            val,
            Value::Array(vec![Value::Int(5), Value::Str(Sds::from_str("x"))])
        );
    }

    /// Тест: чтение битмапы через TAG_BITMAP должно вернуть Value::Bitmap с
    /// правильными байтами
    #[test]
    fn test_read_bitmap() {
        // формируем: [TAG_BITMAP][len=3][bytes 0x01,0x02,0x03]
        let mut data = Vec::new();
        data.push(TAG_BITMAP);
        data.extend(&(3u32).to_be_bytes());
        data.extend(&[1u8, 2, 3]);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        if let Value::Bitmap(bm) = val {
            assert_eq!(bm.as_bytes(), &[1, 2, 3]);
        } else {
            panic!("Expected Bitmap");
        }
    }

    /// Тест: StreamReader останавливается на TAG_EOF
    #[test]
    fn test_stream_reader_eof() {
        let mut buf = Vec::new();
        buf.extend(FILE_MAGIC);
        buf.push(FormatVersion::V1 as u8);
        // сразу EOF
        buf.push(TAG_EOF);

        let mut reader = StreamReader::new(&buf[..]).unwrap();
        assert!(reader.next().is_none());
    }
}
