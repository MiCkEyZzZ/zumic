//! Модуль для десериализации значений `Value` из бинарного формата.
//!
//! Поддерживаются все внутренние типы данных базы:
//! строки, числа, множества, словари, ZSet, HyperLogLog и Stream.
//!
//! Каждое значение начинается с однобайтового тега, за которым следует длина и данные.

use std::{
    collections::HashSet,
    io::{self, Error, ErrorKind, Read},
};

use byteorder::{BigEndian, ReadBytesExt};
use ordered_float::OrderedFloat;

use super::{
    decompress_block, DUMP_VERSION, FILE_MAGIC, TAG_BOOL, TAG_COMPRESSED, TAG_EOF, TAG_FLOAT,
    TAG_HASH, TAG_HLL, TAG_INT, TAG_NULL, TAG_SET, TAG_STR, TAG_ZSET,
};
use crate::{Dict, Hll, Sds, SkipList, SmartHash, Value, DENSE_SIZE};

/// Итератор по парам <Key, Value> из потокового дампа.
///
/// Будет читать из `r` по одной записи, пока не встретит `TAG_EOF`.
pub struct StreamReader<R: Read> {
    inner: R,
    done: bool,
}

impl<R: Read> StreamReader<R> {
    /// Создаёт новый stream-reader, проверяя заголовок.
    pub fn new(mut r: R) -> std::io::Result<Self> {
        let mut magic = [0; 3];
        r.read_exact(&mut magic)?;
        if &magic != FILE_MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad magic"));
        }
        let ver = r.read_u8()?;
        if ver != DUMP_VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad version"));
        }
        Ok(Self {
            inner: r,
            done: false,
        })
    }
}

/// Десериализует значение [`Value`] из бинарного потока.
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
        TAG_COMPRESSED => {
            let len = r.read_u32::<BigEndian>()? as usize;
            let mut compressed = vec![0; len];
            r.read_exact(&mut compressed)?;
            let decompressed = decompress_block(&compressed)?;
            let mut slice = decompressed.as_slice();
            read_value(&mut slice)
        }
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

pub fn read_dump<R: Read>(r: &mut R) -> std::io::Result<Vec<(Sds, Value)>> {
    // 1) Читаем и проверяем магию
    let mut magic = [0u8; 3];
    r.read_exact(&mut magic)?;
    if &magic != FILE_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Bad file magic: {magic:?}"),
        ));
    }

    // 2) Читаем версию
    let ver = r.read_u8()?;
    if ver != DUMP_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unsupported dump version: {ver}"),
        ));
    }

    // 3) Читаем число записей
    let count = r.read_u32::<BigEndian>()? as usize;
    let mut items = Vec::with_capacity(count);

    // 4) Десериализуем пары
    for _ in 0..count {
        let klen = r.read_u32::<BigEndian>()? as usize;
        let mut kb = vec![0; klen];
        r.read_exact(&mut kb)?;
        let key = Sds::from_vec(kb);

        let val = read_value(r)?;
        items.push((key, val));
    }

    Ok(items)
}

impl<R: Read> Iterator for StreamReader<R> {
    type Item = std::io::Result<(Sds, Value)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        // Читаем следующий байт, чтобы узнать: это EOF или длина ключа?
        let mut peek = [0u8; 1];
        if let Err(e) = self.inner.read_exact(&mut peek) {
            return Some(Err(e));
        }
        if peek[0] == TAG_EOF {
            self.done = true;
            return None;
        }

        // Это первая байт длины ключа (big-endian u32), а не EOF.
        // Так как мы уже съели первый байт длины, соберём полный u32:
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

        // Десериализуем само значение
        match read_value(&mut self.inner) {
            Ok(val) => Some(Ok((key, val))),
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{compress_block, write_dump, write_stream};

    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_str() {
        let s = b"hello";
        let mut data = Vec::new();
        data.push(TAG_STR);
        data.extend(&(s.len() as u32).to_be_bytes());
        data.extend(s);

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(b"hello".to_vec())));
    }

    #[test]
    fn test_read_empty_str() {
        let mut data = vec![TAG_STR];
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(Vec::new())));
    }

    #[test]
    fn test_read_int() {
        let i = -123456i64;
        let mut data = Vec::new();
        data.push(TAG_INT);
        data.extend(&i.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        assert_eq!(val, Value::Int(i));
    }

    #[test]
    fn test_read_float() {
        use std::f64::consts::PI;

        let f = PI;
        let mut data = Vec::new();
        data.push(TAG_FLOAT);
        data.extend(&f.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        match val {
            Value::Float(v) => assert!((v - f).abs() < 1e-10),
            _ => panic!("Expected Value::Float"),
        }
    }

    #[test]
    fn test_read_bool_true() {
        let data = vec![TAG_BOOL, 1];

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        assert_eq!(val, Value::Bool(true));
    }

    #[test]
    fn test_read_bool_false() {
        let data = vec![TAG_BOOL, 0];

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        assert_eq!(val, Value::Bool(false));
    }

    #[test]
    fn test_read_null() {
        let data = vec![TAG_NULL];

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        assert_eq!(val, Value::Null);
    }

    #[test]
    fn test_read_hash_empty() {
        let mut data = Vec::new();
        data.push(TAG_HASH);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        match val {
            Value::Hash(map) => assert!(map.is_empty()),
            _ => panic!("Expected Value::Hash"),
        }
    }

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
        let val = read_value(&mut cursor).unwrap();
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
        let err = read_value(&mut cursor).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("Expected Str for Hash value"));
    }

    #[test]
    fn test_read_zset_empty() {
        let mut data = Vec::new();
        data.push(TAG_ZSET);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        match val {
            Value::ZSet { dict, sorted } => {
                assert!(dict.is_empty());
                assert!(sorted.is_empty());
            }
            _ => panic!("Expected Value::ZSet"),
        }
    }

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
        let val = read_value(&mut cursor).unwrap();
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

    #[test]
    fn test_read_set_empty() {
        let mut data = Vec::new();
        data.push(TAG_SET);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();
        match val {
            Value::Set(set) => assert!(set.is_empty()),
            _ => panic!("Expected Value::Set"),
        }
    }

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
        let val = read_value(&mut cursor).unwrap();
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

    #[test]
    fn test_read_hll_with_less_than_dense_size() {
        let n = 2usize; // меньше DENSE_SIZE
        let regs = vec![1u8, 2u8];

        let mut data = Vec::new();
        data.push(TAG_HLL);
        data.extend(&(n as u32).to_be_bytes());
        data.extend(&regs);

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();

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

    #[test]
    fn test_read_hll_with_exact_dense_size() {
        let regs = vec![7u8; DENSE_SIZE];

        let mut data = Vec::new();
        data.push(TAG_HLL);
        data.extend(&(DENSE_SIZE as u32).to_be_bytes());
        data.extend(&regs);

        let mut cursor = Cursor::new(data);
        let val = read_value(&mut cursor).unwrap();

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

    #[test]
    fn test_read_unknown_tag_error() {
        let data = vec![255]; // несуществующий тег

        let mut cursor = Cursor::new(data);
        let err = read_value(&mut cursor).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("Unknown tag"));
    }

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
        let val = super::read_value(&mut cursor).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(raw.to_vec())));
    }

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

    #[test]
    fn test_read_dump_bad_magic() {
        let mut buf = Vec::new();
        // неправильная магия
        buf.extend(b"BAD");
        buf.push(super::DUMP_VERSION);
        buf.extend(&0u32.to_be_bytes());
        let err = super::read_dump(&mut &buf[..]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_read_dump_wrong_version() {
        let mut buf = Vec::new();
        buf.extend(super::FILE_MAGIC);
        buf.push(super::DUMP_VERSION + 1); // не та версия
        buf.extend(&0u32.to_be_bytes());
        let err = super::read_dump(&mut &buf[..]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
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
