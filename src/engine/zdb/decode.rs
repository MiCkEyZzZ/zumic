//! Модуль для десериализации значений `Value` из бинарного формата.
//!
//! Поддерживаются все внутренние типы данных базы: строки, числа, множества,
//! словари, ZSet, HyperLogLog и Stream.
//!
//! Каждое значение начинается с однобайтового тега, за которым следует длина и
//! данные.

use std::{
    collections::HashSet,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};

use byteorder::{BigEndian, ReadBytesExt};
use crc32fast::Hasher;
use ordered_float::OrderedFloat;
use zumic_error::{ensure, ResultExt, ZdbError, ZdbVersionError, ZumicResult};

use super::{
    streaming::{CollectHandler, StreamingParser},
    CompatibilityInfo, Crc32Read, FormatVersion, VersionUtils, FILE_MAGIC, TAG_ARRAY, TAG_BITMAP,
    TAG_BOOL, TAG_COMPRESSED, TAG_EOF, TAG_FLOAT, TAG_HASH, TAG_HLL, TAG_INT, TAG_NULL, TAG_SET,
    TAG_STR, TAG_ZSET,
};
use crate::{
    database::{Bitmap, HllDense, HllEncoding, MurmurHasher, SERIALIZATION_VERSION},
    engine::varint,
    Dict, Hll, Sds, SkipList, SmartHash, Value,
};

const DENSE_SIZE: usize = 16 * 1024;
// Константы безопасности для предотвращения атак через огромные размеры
const MAX_COMPRESSED_SIZE: u32 = 100 * 1024 * 1024; // 100 MB
const MAX_STRING_SIZE: u32 = 512 * 1024 * 1024; // 512 MB
const MAX_COLLECTION_SIZE: u32 = 10_000_000; // 10M элементов
const MAX_BITMAP_SIZE: u32 = 100 * 1024 * 1024; // 100 MB

/// Итератор по парам <Key, Value> из потокового дампа.
///
/// Будет читать из `r` по одной записи, пока не встретит `TAG_EOF`.
pub struct StreamReader<R: Read> {
    inner: R,
    version: FormatVersion,
    done: bool,
    compatibility_info: CompatibilityInfo,
    bytes_read: u64,
}

impl<R: Read> StreamReader<R> {
    /// Создаёт новый stream-reader с проверкой совместимости версий.
    pub fn new(r: R) -> ZumicResult<Self> {
        Self::new_with_version(r, FormatVersion::current())
    }

    /// Создаёт stream-reader с явно указанной версией читателя.
    pub fn new_with_version(
        mut r: R,
        reader_version: FormatVersion,
    ) -> ZumicResult<Self> {
        let start_pos = 0u64;
        let mut magic = [0; 3];

        r.read_exact(&mut magic)
            .context("Failed to read magic number")?;

        ensure!(
            &magic == FILE_MAGIC,
            ZdbError::InvalidMagic {
                expected: *FILE_MAGIC,
                got: magic
            }
        );

        // Читаем версию дампа
        let version_byte = r.read_u8().context("Failed to read version byte")?;
        let dump_version = FormatVersion::try_from(version_byte).map_err(|_| {
            ZdbError::Version(ZdbVersionError::UnsupportedVersion {
                found: version_byte,
                supported: vec![0, 1, 2, 3],
                offset: Some(start_pos + 3),
                key: None,
            })
        })?;

        // Проверяем совместимость
        let compatibility_info = VersionUtils::validate_compatibility(reader_version, dump_version)
            .map_err(ZdbError::from)?;

        Ok(Self {
            inner: r,
            version: dump_version,
            done: false,
            compatibility_info,
            bytes_read: 4,
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

    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

impl<R: Read> Iterator for StreamReader<R> {
    type Item = ZumicResult<(Sds, Value)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let offset = self.bytes_read;

        // Проверяем EOF
        let mut peek = [0u8; 1];
        match self.inner.read_exact(&mut peek) {
            Ok(_) => self.bytes_read += 1,
            Err(_) => {
                return Some(Err(ZdbError::UnexpectedEof {
                    context: "reading next entry".to_string(),
                    offset: Some(offset),
                    key: None,
                    expected_bytes: Some(1),
                    got_bytes: Some(0),
                }
                .into()))
            }
        }

        if peek[0] == TAG_EOF {
            self.done = true;
            return None;
        }

        // Читаем длину ключа (varint или u32)
        let klen = if self.version.uses_varint() {
            // V3: первый байт уже прочитан в peek[0]
            // Используем вспомогательную структуру для чтения продолжения
            let mut combined = std::io::Cursor::new([peek[0]]);
            let partial = varint::read_varint(&mut combined);

            match partial {
                Ok(len) => {
                    // Varint завершился на первом байте
                    len
                }
                Err(_) => {
                    // Нужно читать продолжение
                    // Вернём peek[0] обратно через chain
                    let combined_reader = std::io::Cursor::new([peek[0]]).chain(&mut self.inner);
                    match varint::read_varint(&mut std::io::BufReader::new(combined_reader)) {
                        Ok(len) => {
                            // Обновляем счётчик (уже учли 1 байт выше)
                            // varint может занять больше байт
                            len
                        }
                        Err(e) => return Some(Err(e)),
                    }
                }
            }
        } else {
            // V1/V2: фиксированный u32
            let mut len_buf = [0u8; 4];
            len_buf[0] = peek[0];
            if self.inner.read_exact(&mut len_buf[1..]).is_err() {
                return Some(Err(ZdbError::UnexpectedEof {
                    context: "reading key length".to_string(),
                    offset: Some(offset),
                    key: None,
                    expected_bytes: Some(4),
                    got_bytes: Some(1),
                }
                .into()));
            }
            self.bytes_read += 3;
            u32::from_be_bytes(len_buf)
        };

        // Валидация длины ключа
        if klen > MAX_STRING_SIZE {
            return Some(Err(ZdbError::SizeLimit {
                what: "Key".to_string(),
                size: klen as u64,
                limit: MAX_STRING_SIZE as u64,
                offset: Some(offset),
                key: None,
            }
            .into()));
        }

        let mut kb = vec![0; klen as usize];
        if let Err(e) = self.inner.read_exact(&mut kb) {
            return Some(Err(ZdbError::from(e).into()));
        }
        self.bytes_read += klen as u64;

        let key = Sds::from_vec(kb);
        let key_str = String::from_utf8_lossy(key.as_bytes()).to_string();

        // Читаем значение
        match read_value_with_version(&mut self.inner, self.version, Some(&key_str), offset) {
            Ok(v) => Some(Ok((key, v))),
            Err(e) => {
                if let Some(zdb_err) = e.downcast_ref::<ZdbError>() {
                    let updated = zdb_err.clone().with_key(&key_str);
                    return Some(Err(updated.into()));
                }
                Some(Err(e))
            }
        }
    }
}

/// Десериализует значение `Value`] из бинарного потока.
pub fn read_value<R: Read>(r: &mut R) -> ZumicResult<Value> {
    read_value_with_version(r, FormatVersion::current(), None, 0)
}

/// Десериализует значение с явной версией формата и контекстом.
pub fn read_value_with_version<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let tag = r.read_u8().map_err(|_| ZdbError::UnexpectedEof {
        context: "reading value tag".to_string(),
        offset: Some(offset),
        key: key.map(|s| s.to_string()),
        expected_bytes: Some(1),
        got_bytes: Some(0),
    })?;

    let result = match tag {
        TAG_STR => read_string_value(r, version, key, offset),
        TAG_INT => read_int_value(r, version, key, offset),
        TAG_FLOAT => read_float_value(r, version, key, offset),
        TAG_BOOL => read_bool_value(r, version, key, offset),
        TAG_NULL => Ok(Value::Null),
        TAG_COMPRESSED => read_compressed_value(r, version, key, offset),
        TAG_HASH => read_hash_value(r, version, key, offset),
        TAG_ZSET => read_zset_value(r, version, key, offset),
        TAG_SET => read_set_value(r, version, key, offset),
        TAG_HLL => read_hll_value(r, version, key, offset),
        TAG_ARRAY => read_array_value(r, version, key, offset),
        TAG_BITMAP => read_bitmap_value(r, version, key, offset),
        other => Err(ZdbError::InvalidTag {
            tag: other,
            offset: Some(offset),
            key: key.map(|s| s.to_string()),
            valid_tags: vec![
                TAG_STR,
                TAG_INT,
                TAG_FLOAT,
                TAG_BOOL,
                TAG_NULL,
                TAG_COMPRESSED,
                TAG_HASH,
                TAG_ZSET,
                TAG_SET,
                TAG_HLL,
                TAG_ARRAY,
                TAG_BITMAP,
            ],
        }
        .into()),
    };

    result.map_err(|e| {
        if let Some(zdb_err) = e.downcast_ref::<ZdbError>() {
            if let Some(k) = key {
                let updated = zdb_err.clone().with_key(k);
                return updated.into();
            }
        }
        e
    })
}

/// read_dump (legacy wrapper) - сохраняем для обратной совместимости
pub fn read_dump<R: Read>(r: &mut R) -> ZumicResult<Vec<(Sds, Value)>> {
    read_dump_with_version(r, FormatVersion::current())
}

/// Попытка streaming-safe чтения дампа из файла (без read_to_end()).
/// Для файлов: читает тело (file_len - 4) через Crc32Read, парсит, затем
/// сверяет CRC.
pub fn read_dump_streaming_file(path: &str) -> ZumicResult<Vec<(Sds, Value)>> {
    let mut file = File::open(path).context("Failed ti open dump file")?;
    let file_size = file
        .metadata()
        .context("Failed to read file metadata")?
        .len();

    ensure!(
        file_size >= 8,
        ZdbError::FileTooSmall {
            size: file_size,
            minimum: 8
        }
    );

    let body_len = file_size - 4;

    // Создаём take поверх клона файла, чтобы не двигать основную позицию
    let take = file
        .try_clone()
        .context("Failed to clone file handle")?
        .take(body_len);
    let crc_reader = Crc32Read::new(take);

    // Парсим через streaming parser (парсер добавит BufReader)
    let mut parser = StreamingParser::new(crc_reader)?;
    let mut handler = CollectHandler::new();
    parser.parse(&mut handler)?;

    // Извлекаем внутренний reader чтобы получить computed CRC
    let buf_reader = parser.into_inner();
    let crc_wrapped = buf_reader.into_inner();
    let (_take_back, computed_crc) = crc_wrapped.into_inner_and_finalize();

    // Читаем записанный CRC из конца файла
    file.seek(SeekFrom::End(-4))
        .context("Failed to seek to CRC")?;
    let recorded_crc = file.read_u32::<BigEndian>().context("Failed to read CRC")?;

    ensure!(
        computed_crc == recorded_crc,
        ZdbError::CrcMismatch {
            computed: computed_crc,
            recorded: recorded_crc,
            offset: Some(file_size - 4),
        }
    );

    Ok(handler.into_items())
}

/// Читает дамп с явно указанной версией читателя.
///
/// По возможности использует streaming-first подход; для потоков без seek -
/// fall back на legacy (read_to_end) чтобы сохранить поведение.
pub fn read_dump_with_version<R: Read>(
    r: &mut R,
    reader_version: FormatVersion,
) -> ZumicResult<Vec<(Sds, Value)>> {
    let mut data = Vec::new();
    r.read_to_end(&mut data)
        .context("Failed to read dump data")?;

    ensure!(
        data.len() >= 8,
        ZdbError::FileTooSmall {
            size: data.len() as u64,
            minimum: 8
        }
    );

    let body_len = data.len() - 4;
    let (body, crc_bytes) = data.split_at(body_len);
    let recorded_crc = (&crc_bytes[..4])
        .read_u32::<BigEndian>()
        .context("Failed to read CRC")?;

    let mut hasher = Hasher::new();
    hasher.update(body);
    let computed_crc = hasher.finalize();

    ensure!(
        computed_crc == recorded_crc,
        ZdbError::CrcMismatch {
            computed: computed_crc,
            recorded: recorded_crc,
            offset: Some(body_len as u64)
        }
    );

    // Попытаемся streaming-парсинг тела (cursor)
    let body_vec = body.to_vec();
    {
        let cursor = io::Cursor::new(body_vec.clone());
        if let Ok(mut parser) = StreamingParser::new_with_version(cursor, reader_version) {
            let mut handler = CollectHandler::new();
            if parser.parse(&mut handler).is_ok() {
                return Ok(handler.into_items());
            }
        }
    }

    // Fallback: legacy parsing (count-based)
    {
        let mut cursor = io::Cursor::new(body_vec);
        // читаем magic
        let mut magic = [0u8; 3];
        cursor
            .read_exact(&mut magic)
            .context("Failed to read magic")?;

        ensure!(
            &magic == FILE_MAGIC,
            ZdbError::InvalidMagic {
                expected: *FILE_MAGIC,
                got: magic
            }
        );

        // читаем и валидируем версию
        let version_byte = cursor.read_u8().context("Failed to read version")?;
        let dump_version = FormatVersion::try_from(version_byte).map_err(|_| {
            ZdbError::Version(ZdbVersionError::UnsupportedVersion {
                found: version_byte,
                supported: vec![0, 1, 2, 3],
                offset: Some(3),
                key: None,
            })
        })?;

        let _compat = VersionUtils::validate_compatibility(reader_version, dump_version)
            .map_err(ZdbError::from)?;

        // читаем count
        let count = read_length(&mut cursor, dump_version)?;

        ensure!(
            count <= MAX_COLLECTION_SIZE,
            ZdbError::SizeLimit {
                what: "Dump item count".to_string(),
                size: count as u64,
                limit: MAX_COLLECTION_SIZE as u64,
                offset: Some(4),
                key: None
            }
        );

        let mut items = Vec::with_capacity(count as usize);

        for i in 0..count {
            let offset = cursor.position();

            let klen = read_length(&mut cursor, dump_version)
                .with_context(|| format!("Failed to read key length for item {i}"))?;

            ensure!(
                klen <= MAX_STRING_SIZE,
                ZdbError::SizeLimit {
                    what: "Key length".to_string(),
                    size: klen as u64,
                    limit: MAX_STRING_SIZE as u64,
                    offset: Some(offset),
                    key: None
                }
            );

            let mut kb = vec![0u8; klen as usize];
            cursor
                .read_exact(&mut kb)
                .with_context(|| format!("Failed to read key bytes for item {i}"))?;

            let key = Sds::from_vec(kb);
            let key_str = String::from_utf8_lossy(key.as_bytes()).to_string();

            let val = read_value_with_version(&mut cursor, dump_version, Some(&key_str), offset)
                .with_context(|| format!("Failed to read value for key '{key_str}'"))?;
            items.push((key, val));
        }

        Ok(items)
    }
}

/// Пропускает значение без десериализации (для фильтров и счётчиков).
///
/// Не аллоцирует память для значения - просто пропускает байты.
///
/// Примечание: для [`TAG_COMPRESSED`] мы пропускаем compressed blob (не
/// распаковываем).
pub fn skip_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> ZumicResult<()> {
    let tag = r.read_u8().context("Failed to read tag for skip")?;

    match tag {
        TAG_NULL => Ok(()),
        TAG_BOOL => {
            r.read_u8().context("Failed to skip bool value")?;
            Ok(())
        }
        TAG_INT => {
            r.read_i64::<BigEndian>()
                .context("Failed to skip int value")?;
            Ok(())
        }
        TAG_FLOAT => {
            r.read_f64::<BigEndian>()
                .context("Failed to skip float value")?;
            Ok(())
        }
        TAG_STR => {
            let len = read_length(r, version)? as u64;
            skip_bytes(r, len)?;
            Ok(())
        }
        TAG_COMPRESSED => {
            let len = read_length(r, version)? as u64;
            skip_bytes(r, len)?;
            Ok(())
        }
        TAG_ARRAY => {
            let count = read_length(r, version)?;
            for _ in 0..count {
                skip_value(r, version)?;
            }
            Ok(())
        }
        TAG_HASH => {
            let count = read_length(r, version)?;
            for _ in 0..count {
                let key_len = read_length(r, version)? as u64;
                skip_bytes(r, key_len)?;
                skip_value(r, version)?;
            }
            Ok(())
        }
        TAG_SET => {
            let count = read_length(r, version)?;
            for _ in 0..count {
                let elem_len = read_length(r, version)? as u64;
                skip_bytes(r, elem_len)?;
            }
            Ok(())
        }
        TAG_ZSET => {
            let count = read_length(r, version)?;
            for _ in 0..count {
                let key_len = read_length(r, version)? as u64;
                skip_bytes(r, key_len)?;
                match version {
                    FormatVersion::Legacy => {
                        r.read_f32::<BigEndian>()
                            .context("Failed to skip zset score (f32)")?;
                    }
                    FormatVersion::V1 | FormatVersion::V2 | FormatVersion::V3 => {
                        r.read_f64::<BigEndian>()
                            .context("Failed to skip zset score (f64)")?;
                    }
                }
            }
            Ok(())
        }
        TAG_HLL => {
            let len = read_length(r, version)? as u64;
            skip_bytes(r, len)?;
            Ok(())
        }
        TAG_BITMAP => {
            let len = read_length(r, version)? as u64;
            skip_bytes(r, len)?;
            Ok(())
        }
        other => Err(ZdbError::InvalidTag {
            tag: other,
            offset: None,
            key: None,
            valid_tags: vec![
                TAG_STR,
                TAG_INT,
                TAG_FLOAT,
                TAG_BOOL,
                TAG_NULL,
                TAG_COMPRESSED,
                TAG_HASH,
                TAG_ZSET,
                TAG_SET,
                TAG_HLL,
                TAG_ARRAY,
                TAG_BITMAP,
            ],
        }
        .into()),
    }
}

// ============================================================================
// Функции чтения отдельных типов значений (для read_value_with_version)
// ============================================================================

fn read_string_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let len = read_length(r, version)?;

    ensure!(
        len <= MAX_STRING_SIZE,
        ZdbError::SizeLimit {
            what: "String".to_string(),
            size: len as u64,
            limit: MAX_STRING_SIZE as u64,
            offset: Some(offset),
            key: key.map(|s| s.to_string())
        }
    );

    let mut buf = vec![0; len as usize];
    r.read_exact(&mut buf)
        .context("Failed to read string bytes")?;
    Ok(Value::Str(Sds::from_vec(buf)))
}

fn read_int_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
    _key: Option<&str>,
    _offset: u64,
) -> ZumicResult<Value> {
    let i = r
        .read_i64::<BigEndian>()
        .context("Failed to read int value")?;
    Ok(Value::Int(i))
}

fn read_float_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
    _key: Option<&str>,
    _offset: u64,
) -> ZumicResult<Value> {
    let f = r
        .read_f64::<BigEndian>()
        .context("Failed to read float value")?;
    Ok(Value::Float(f))
}

fn read_bool_value<R: Read>(
    r: &mut R,
    _version: FormatVersion,
    _key: Option<&str>,
    _offset: u64,
) -> ZumicResult<Value> {
    let b = r.read_u8().context("Failed to read bool value")? != 0;
    Ok(Value::Bool(b))
}

fn read_compressed_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let len = read_length(r, version)?;

    ensure!(
        len > 0,
        ZdbError::CompressionError {
            operation: zumic_error::CompressionOp::Decompress,
            reason: "Compressed block cannot be empty".to_string(),
            offset: Some(offset),
            key: key.map(|s| s.to_string()),
            compressed_size: Some(len)
        }
    );

    ensure!(
        len <= MAX_COMPRESSED_SIZE,
        ZdbError::SizeLimit {
            what: "Compressed data".to_string(),
            size: len as u64,
            limit: MAX_COMPRESSED_SIZE as u64,
            offset: Some(offset),
            key: key.map(|s| s.to_string())
        }
    );

    let limited = r.take(len as u64);

    let decoder = zstd::stream::Decoder::new(limited).map_err(|e| ZdbError::CompressionError {
        operation: zumic_error::CompressionOp::Decompress,
        reason: format!("zstd decoder error: {e}"),
        offset: Some(offset),
        key: key.map(|s| s.to_string()),
        compressed_size: Some(len),
    })?;

    let mut boxed_reader: Box<dyn Read> = Box::new(std::io::BufReader::new(decoder));

    let val = read_value_with_version(&mut boxed_reader.as_mut(), version, key, offset)
        .context("Failed to read compressed value")?;

    std::io::copy(&mut boxed_reader.as_mut(), &mut std::io::sink())
        .context("Failed to drain compressed stream")?;

    Ok(val)
}

fn read_hash_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let n = read_length(r, version)?;

    ensure!(
        n <= MAX_COLLECTION_SIZE,
        ZdbError::SizeLimit {
            what: "Hash".to_string(),
            size: n as u64,
            limit: MAX_COLLECTION_SIZE as u64,
            offset: Some(offset),
            key: key.map(|s| s.to_string())
        }
    );

    let mut map = SmartHash::new();

    for i in 0..n {
        let klen = read_length(r, version)
            .with_context(|| format!("Failed to read hash key length at index {i}"))?;

        ensure!(
            klen <= MAX_STRING_SIZE,
            ZdbError::SizeLimit {
                what: format!("Hash key at index {i}"),
                size: klen as u64,
                limit: MAX_STRING_SIZE as u64,
                offset: Some(offset),
                key: key.map(|s| s.to_string())
            }
        );

        let mut kb = vec![0; klen as usize];
        r.read_exact(&mut kb)
            .with_context(|| format!("Failed to read hash key bytes at index {i}"))?;
        let entry_key = Sds::from_vec(kb);
        let entry_key_str = String::from_utf8_lossy(entry_key.as_bytes()).to_string();

        let raw = read_value_with_version(r, version, Some(&entry_key_str), offset).with_context(
            || format!("Failed to read hash value at index {i} (entry key: {entry_key_str})",),
        )?;

        let val = match raw {
            Value::Str(s) => s,
            _ => {
                return Err(ZdbError::ParseError {
                    structure: "Hash".to_string(),
                    reason: format!(
                        "Expected Str for Hash value at index {i} (entry key: {entry_key_str})",
                    ),
                    offset: Some(offset),
                    key: key.map(|s| s.to_string()),
                }
                .into());
            }
        };

        map.insert(entry_key, val);
    }

    Ok(Value::Hash(map))
}

fn read_zset_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let n = read_length(r, version)?;

    ensure!(
        n <= MAX_COLLECTION_SIZE,
        ZdbError::SizeLimit {
            what: "Zset".to_string(),
            size: n as u64,
            limit: MAX_COLLECTION_SIZE as u64,
            offset: Some(offset),
            key: key.map(|s| s.to_string())
        }
    );

    let mut dict = Dict::new();
    let mut sorted = SkipList::new();

    for i in 0..n {
        let klen = read_length(r, version)
            .with_context(|| format!("Failed to read zset key length at index {i}"))?;

        ensure!(
            klen <= MAX_STRING_SIZE,
            ZdbError::SizeLimit {
                what: format!("ZSet key at index {i}"),
                size: klen as u64,
                limit: MAX_STRING_SIZE as u64,
                offset: Some(offset),
                key: key.map(|s| s.to_string())
            }
        );

        let mut kb = vec![0; klen as usize];
        r.read_exact(&mut kb)
            .with_context(|| format!("Failed to read zset key bytes at index {i}"))?;
        let key = Sds::from_vec(kb);

        let score = match version {
            FormatVersion::Legacy => r
                .read_f32::<BigEndian>()
                .with_context(|| format!("Failed to read zset score (f32) at index {i}"))?
                as f64,
            FormatVersion::V1 | FormatVersion::V2 | FormatVersion::V3 => r
                .read_f64::<BigEndian>()
                .with_context(|| format!("Failed to read zset score (f64) at index {i}"))?,
        };

        dict.insert(key.clone(), score);
        sorted.insert(OrderedFloat(score), key);
    }

    Ok(Value::ZSet { dict, sorted })
}

fn read_set_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let n = read_length(r, version)?;

    ensure!(
        n <= MAX_COLLECTION_SIZE,
        ZdbError::SizeLimit {
            what: "Set".into(),
            size: n as u64,
            limit: MAX_COLLECTION_SIZE as u64,
            offset: Some(offset),
            key: key.map(|s| s.to_string()),
        }
    );

    let mut set = HashSet::new();

    for i in 0..n {
        let klen = read_length(r, version)
            .with_context(|| format!("Failed to read set element length at index {i}"))?;

        ensure!(
            klen <= MAX_STRING_SIZE,
            ZdbError::SizeLimit {
                what: format!("Set element at index {i}"),
                size: klen as u64,
                limit: MAX_STRING_SIZE as u64,
                offset: Some(offset),
                key: key.map(|s| s.to_string())
            }
        );

        let mut kb = vec![0; klen as usize];
        r.read_exact(&mut kb)
            .with_context(|| format!("Failed to read set element bytes at index {i}"))?;
        set.insert(Sds::from_vec(kb));
    }

    Ok(Value::Set(set))
}

fn read_hll_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let n = read_length(r, version)?;

    match version {
        FormatVersion::Legacy => {
            ensure!(
                n <= DENSE_SIZE as u32,
                ZdbError::SizeLimit {
                    what: "HLL (legasy)".to_string(),
                    size: n as u64,
                    limit: DENSE_SIZE as u64,
                    offset: Some(offset),
                    key: key.map(|s| s.to_string())
                }
            );
        }
        FormatVersion::V1 | FormatVersion::V2 | FormatVersion::V3 => {
            ensure!(
                n <= DENSE_SIZE as u32 * 2,
                ZdbError::SizeLimit {
                    what: "HLL".to_string(),
                    size: n as u64,
                    limit: (DENSE_SIZE * 2) as u64,
                    offset: Some(offset),
                    key: key.map(|s| s.to_string())
                }
            );
        }
    }

    let mut regs = vec![0u8; n as usize];
    r.read_exact(&mut regs).context("Failed to read HLL data")?;

    // Создаём плотный массив и копируем прочитанные биты
    let mut data = [0u8; DENSE_SIZE];
    let to_copy = std::cmp::min(regs.len(), DENSE_SIZE);
    data[..to_copy].copy_from_slice(&regs[..to_copy]);

    // Преобразуем в HllDense и возвращаем новый Hll с Dense encoding
    let dense = HllDense {
        data: data.to_vec(),
    };
    let hll = Hll {
        encoding: HllEncoding::Dense(Box::new(dense)),
        version: SERIALIZATION_VERSION,
        hasher: MurmurHasher::default(),
    };

    Ok(Value::HyperLogLog(Box::new(hll)))
}

fn read_array_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let len = read_length(r, version)?;

    ensure!(
        len <= MAX_COLLECTION_SIZE,
        ZdbError::SizeLimit {
            what: "Array".to_string(),
            size: len as u64,
            limit: MAX_COLLECTION_SIZE as u64,
            offset: Some(offset),
            key: key.map(|s| s.to_string())
        }
    );

    let mut items = Vec::with_capacity(len as usize);

    for i in 0..len {
        let item = read_value_with_version(r, version, key, offset)
            .with_context(|| format!("Failed to read array element at index {i}"))?;
        items.push(item)
    }

    Ok(Value::Array(items))
}

fn read_bitmap_value<R: Read>(
    r: &mut R,
    version: FormatVersion,
    key: Option<&str>,
    offset: u64,
) -> ZumicResult<Value> {
    let byte_len = read_length(r, version)?;

    match version {
        FormatVersion::Legacy => {
            ensure!(
                byte_len <= 1024 * 1024,
                ZdbError::SizeLimit {
                    what: "Bitmap (legacy)".to_string(),
                    size: byte_len as u64,
                    limit: 1024 * 1024,
                    offset: Some(offset),
                    key: key.map(|s| s.to_string())
                }
            );
        }
        FormatVersion::V1 | FormatVersion::V2 | FormatVersion::V3 => {
            ensure!(
                byte_len <= MAX_BITMAP_SIZE,
                ZdbError::SizeLimit {
                    what: "Bitmap".to_string(),
                    size: byte_len as u64,
                    limit: MAX_BITMAP_SIZE as u64,
                    offset: Some(offset),
                    key: key.map(|s| s.to_string())
                }
            );
        }
    }

    let mut buf = vec![0u8; byte_len as usize];
    r.read_exact(&mut buf)
        .context("Failed to read bitmap data")?;

    let mut bmp = Bitmap::new();
    bmp.bytes = buf;

    Ok(Value::Bitmap(bmp))
}

// ============================================================================
// Skip utilities - пропуск значений без десериализации
// ============================================================================

/// Пропускает N байт без аллокаций без десериализации (прочитай и выкинь).
///
/// Возвращает ошибку [`UnexpectedEof`], если в потоке недостаточно байт.
fn skip_bytes<R: Read>(
    r: &mut R,
    mut n: u64,
) -> ZumicResult<()> {
    let mut buf = [0u8; 8 * 1024];

    while n > 0 {
        let to_read = std::cmp::min(n, buf.len() as u64) as usize;
        let read = r
            .read(&mut buf[..to_read])
            .context("Failed to skip bytes")?;
        if read == 0 {
            // EOF раньше времени — это ошибка (файл усечён)
            return Err(ZdbError::UnexpectedEof {
                context: "slipping bytes".to_string(),
                offset: None,
                key: None,
                expected_bytes: Some(n),
                got_bytes: Some(0),
            }
            .into());
        }
        n -= read as u64;
    }

    Ok(())
}

/// Читает длину: u32 BigEndian (V1/V2) or varint (V3).
#[inline]
fn read_length<R: Read>(
    r: &mut R,
    version: FormatVersion,
) -> ZumicResult<u32> {
    if version.uses_varint() {
        varint::read_varint(r).context("Failed to read varint length") // V3
    } else {
        r.read_u32::<BigEndian>()
            .context("Failed to read fixed length") // V1/V2
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::engine::{compress_block, write_dump, write_stream};

    // Используем V1 для всех тестов, где мы вручную пишем 4-байтовые BE длины,
    // потому что V3 ожидает varint-encoding.
    const LEGACY: FormatVersion = FormatVersion::V1;

    /// Тест проверяет, что чтение строки даст `Value::Str("hello")`
    #[test]
    fn test_read_str() {
        let s = b"hello";
        let mut data = Vec::new();
        data.push(TAG_STR);
        data.extend(&(s.len() as u32).to_be_bytes());
        data.extend(s);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(b"hello".to_vec())));
    }

    /// Тест проверяет, что чтение пустой строки даст `Value::Str("")`
    #[test]
    fn test_read_empty_str() {
        let mut data = vec![TAG_STR];
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        assert_eq!(val, Value::Bool(true));
    }

    /// Тест проверяет, что чтение булевого `false` даст `Value::Bool(false)`
    #[test]
    fn test_read_bool_false() {
        let data = vec![TAG_BOOL, 0];
        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        assert_eq!(val, Value::Bool(false));
    }

    /// Тест проверяет, что чтение `null` даст `Value::Null`
    #[test]
    fn test_read_null() {
        let data = vec![TAG_NULL];
        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        assert_eq!(val, Value::Null);
    }

    /// Тест проверяет, что чтение пустого хеша даст пустой `Value::Hash`
    #[test]
    fn test_read_hash_empty() {
        let mut data = Vec::new();
        data.push(TAG_HASH);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        match val {
            Value::Hash(m) => {
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
        let key = b"key";

        let mut data = Vec::new();
        data.push(TAG_HASH);
        data.extend(&(1u32).to_be_bytes());

        // ключ
        data.extend(&(key.len() as u32).to_be_bytes());
        data.extend(key);

        // значение - INT, а должно быть STR
        data.push(TAG_INT);
        data.extend(&(123i64).to_be_bytes());

        let mut cursor = Cursor::new(data);

        let err = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap_err();

        // Извлекаем ZdbError
        let zdb_err = err.downcast_ref::<ZdbError>().expect("Expected ZdbError");

        // Конвертируем в std::io::Error, чтобы проверить kind()
        let io_err: std::io::Error = zdb_err.clone().into();

        assert_eq!(io_err.kind(), std::io::ErrorKind::InvalidData);
        assert!(io_err.to_string().contains("Expected Str for Hash value"));
    }

    /// Тест проверяет, что чтение пустого ZSet даст пустой `Value::ZSet`
    #[test]
    fn test_read_zset_empty() {
        let mut data = Vec::new();
        data.push(TAG_ZSET);
        data.extend(&(0u32).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        match val {
            Value::ZSet { mut dict, sorted } => {
                assert_eq!(dict.len(), 2);
                assert_eq!(dict.get(&Sds::from_vec(key1.to_vec())), Some(&score1));
                assert_eq!(dict.get(&Sds::from_vec(key2.to_vec())), Some(&score2));
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();

        match val {
            Value::HyperLogLog(hll) => match &hll.encoding {
                HllEncoding::Dense(dense) => {
                    assert_eq!(dense.data[0], 1);
                    assert_eq!(dense.data[1], 2);
                    for i in 2..DENSE_SIZE {
                        assert_eq!(dense.data[i], 0);
                    }
                }
                _ => panic!("Expected HLL with Dense encoding"),
            },
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
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();

        match val {
            Value::HyperLogLog(hll) => match &hll.encoding {
                HllEncoding::Dense(dense) => {
                    assert_eq!(dense.data.len(), DENSE_SIZE);
                    for &b in dense.data.iter() {
                        assert_eq!(b, 7);
                    }
                }
                _ => panic!("Expected HLL with Dense encoding"),
            },
            _ => panic!("Expected Value::HyperLogLog"),
        }
    }

    /// Тест проверяет, что неизвестный тег вызывает ошибку `InvalidData` с
    /// сообщением "Unknown tag"
    #[test]
    fn test_read_unknown_tag_error() {
        let data = vec![255];

        let mut cursor = Cursor::new(data);
        let err =
            read_value_with_version(&mut cursor, FormatVersion::current(), None, 0).unwrap_err();
        let zdb_err = err.downcast_ref::<ZdbError>().expect("Expected ZdbError");
        let io_err: std::io::Error = zdb_err.clone().into();

        assert_eq!(io_err.kind(), std::io::ErrorKind::InvalidData);
        assert!(io_err.to_string().contains("tag") || io_err.to_string().contains("Unknown"));
    }

    #[test]
    fn test_read_compressed_str() {
        let raw =
            b"some longer string that will be compressed because length > MIN_COMPRESSION_SIZE";
        let mut inner = Vec::new();
        inner.push(TAG_STR);
        inner.extend(&(raw.len() as u32).to_be_bytes());
        inner.extend(raw);

        let mut compressed = compress_block(&inner).expect("compress failed");
        if compressed.is_empty() {
            // на всякий случай: если compress_block по каким-то настройкам вернул пусто,
            // сожмём через zstd напрямую (fallback).
            compressed = zstd::bulk::compress(&inner, 0).expect("zstd compress fallback failed");
        }

        let mut data = Vec::new();
        data.push(TAG_COMPRESSED);
        data.extend(&(compressed.len() as u32).to_be_bytes());
        data.extend(&compressed);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        assert_eq!(val, Value::Str(Sds::from_vec(raw.to_vec())));
    }

    #[test]
    fn test_read_array() {
        let mut data = Vec::new();
        data.push(TAG_ARRAY);
        data.extend(&(2u32).to_be_bytes());
        data.push(TAG_INT);
        data.extend(&5i64.to_be_bytes());
        data.push(TAG_STR);
        let s = b"x";
        data.extend(&(s.len() as u32).to_be_bytes());
        data.extend(s);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        assert_eq!(
            val,
            Value::Array(vec![Value::Int(5), Value::Str(Sds::from_str("x"))])
        );
    }

    #[test]
    fn test_read_bitmap() {
        let mut data = Vec::new();
        data.push(TAG_BITMAP);
        data.extend(&(3u32).to_be_bytes());
        data.extend(&[1u8, 2, 3]);

        let mut cursor = Cursor::new(data);
        let val = read_value_with_version(&mut cursor, LEGACY, None, 0).unwrap();
        if let Value::Bitmap(bm) = val {
            assert_eq!(bm.as_bytes(), &[1, 2, 3]);
        } else {
            panic!("Expected Bitmap");
        }
    }

    // ----- tests that rely on write_dump / write_stream: keep using real API -----

    #[test]
    fn test_read_dump_bad_magic() {
        let mut buf = Vec::new();
        buf.extend(b"BAD");
        buf.push(FormatVersion::V1 as u8);
        buf.extend(&0u32.to_be_bytes());
        buf.extend(&0u32.to_be_bytes());

        let err = read_dump(&mut &buf[..]).unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_read_dump_wrong_version() {
        let mut buf = Vec::new();
        buf.extend(FILE_MAGIC);
        buf.push((FormatVersion::V2 as u8) + 1);
        buf.extend(&0u32.to_be_bytes());

        let err = read_dump(&mut &buf[..]).unwrap_err();
        assert!(!err.to_string().is_empty());
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

    #[test]
    fn doc_test_read_dump_crc_mismatch() {
        let items = vec![(Sds::from_str("key"), Value::Int(1))];
        let mut buf = Vec::new();
        write_dump(&mut buf, items.into_iter()).unwrap();

        let len = buf.len();
        buf[len - 1] ^= 0xFF;

        let err = read_dump(&mut &buf[..]).unwrap_err();
        let zdb_err = err.downcast_ref::<ZdbError>().expect("Expected ZdbError");
        let io_err: std::io::Error = zdb_err.clone().into();

        assert_eq!(io_err.kind(), std::io::ErrorKind::InvalidData);
        assert!(io_err.to_string().to_lowercase().contains("crc"));
    }

    #[test]
    fn doc_test_dump_empty_crc() {
        let mut buf = Vec::new();
        write_dump(&mut buf, Vec::<(Sds, Value)>::new().into_iter()).unwrap();
        let got = read_dump(&mut &buf[..]).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn doc_test_read_dump_too_small() {
        let err = read_dump(&mut &b"\x00\x01\x02"[..]).unwrap_err();
        let zdb_err = err.downcast_ref::<ZdbError>().expect("Expected ZdbError");
        let io_err: std::io::Error = zdb_err.clone().into();
        assert_eq!(io_err.kind(), std::io::ErrorKind::InvalidData);
        assert!(io_err.to_string().to_lowercase().contains("file too small"));
    }

    // Compressed-size limit tests (version-agnostic)
    #[test]
    fn test_reject_huge_compressed_size() {
        let malicious_input = vec![0x0D, 0xC9, 0xC9, 0xC9, 0xC9];
        let mut cursor = Cursor::new(&malicious_input);
        let result = read_value_with_version(&mut cursor, FormatVersion::V1, None, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_compressed_size_at_limit() {
        let mut data = Vec::new();
        data.push(TAG_COMPRESSED);
        data.extend(&MAX_COMPRESSED_SIZE.to_be_bytes());
        let mut cursor = Cursor::new(data);
        let result = read_value_with_version(&mut cursor, FormatVersion::V1, None, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_over_limit_compressed() {
        let mut data = Vec::new();
        data.push(TAG_COMPRESSED);
        data.extend(&(MAX_COMPRESSED_SIZE + 1).to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = read_value_with_version(&mut cursor, FormatVersion::V1, None, 0);

        assert!(result.is_err());
        if let Err(e) = result {
            let err_msg = e.to_string().to_lowercase();
            assert!(
                err_msg.contains("compressed")
                    || err_msg.contains("size")
                    || err_msg.contains("too")
            );
        }
    }
}
