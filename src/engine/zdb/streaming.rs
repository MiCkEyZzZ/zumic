//! SAX-style streaming parser для ZDB дампов.
//!
//! Этот модуль предоставляет event-driven архитектуру для парсинга дампов без
//! загрузки всего содержимого в память.
//!
//! # Архитектура
//!
//! Парсер читает дамп порциями и вызывает callback'и для каждого события:
//! - `on_header()` - начало дампа с версией
//! - `on_entry()` - каждая пара ключ-значение
//! - `on_end()` - конец дампа
//! - `on_error()` - ошибка парсинга (опционально recoverable)

use std::io::{self, BufReader, Read, Write};

use byteorder::{BigEndian, WriteBytesExt};
use zumic_error::{ensure, ResultExt, StackError, ZdbError, ZumicResult};

use super::{write_value, CompatibilityInfo, FormatVersion, VersionUtils, FILE_MAGIC, TAG_EOF};
use crate::{engine::read_value_with_version, Sds, Value};

/// Трейт для обработки событий парсинга.
pub trait ParseHandler {
    /// Вызывается для каждого события парсинга.
    fn handle_event(
        &mut self,
        event: ParseEvent,
    ) -> ZumicResult<()>;

    /// Должен ли парсер продолжать работу после recoverable ошибки?
    fn should_continue_on_error(&self) -> bool {
        false
    }

    /// Вызывается в конце парсинга для финализации.
    fn finalize(&mut self) -> ZumicResult<()> {
        Ok(())
    }
}

/// Reader обёртка для вычисления CRC32 на лету.
///
/// Обновляет hasher при каждом чтении, не требуя загрузки всех данных.
pub struct Crc32Read<R: Read> {
    inner: R,
    hasher: crc32fast::Hasher,
    bytes_read: u64,
}

/// События, генерируемые парсером во время обработки дампа.
#[derive(Debug, Clone)]
pub enum ParseEvent {
    /// Начало дампа с информацией о версии
    Header {
        version: FormatVersion,
        compatibility: CompatibilityInfo,
    },
    /// Найдена пара ключ-значение
    Entry { key: Sds, value: Value },
    /// Конец дампа (успешное завершение)
    End,
    /// Ошибка парсинга (может быть recoverable)
    Error {
        error: String,
        key: Option<Sds>,
        offset: Option<u64>,
        recoverable: bool,
    },
}

/// Статистика парсинга дампа.
#[derive(Debug, Clone, Default)]
pub struct ParseStats {
    /// Кол-во байт прочитано
    pub bytes_read: u64,
    /// Кол-во успешно обработанных записей
    pub records_parsed: u64,
    /// Кол-во ошибок парсинга
    pub errors_count: u64,
    /// Кол-во пропущенных записей
    pub skipped_records: u64,
    /// Версия дампа
    pub version: Option<FormatVersion>,
}

/// SAX-style streaming parser для ZDB дампов.
///
/// Читает дамп порциями и вызывает hander для каждого события.
/// Не загружает весь дамп в память.
pub struct StreamingParser<R: Read> {
    reader: BufReader<R>,
    version: Option<FormatVersion>,
    stats: ParseStats,
    reader_version: FormatVersion,
}

/// Handler для сбора всех записей в Vec (обратная совместимость).
///
/// Эквивалентен старому `read_dump()`.
#[derive(Debug, Default)]
pub struct CollectHandler {
    items: Vec<(Sds, Value)>,
}

/// Handler для подсчёта статистики без загрузки значений.
///
/// Собирает информацию о дампе без десериализации значений.
#[derive(Debug, Default)]
pub struct CountHandler {
    total_entries: u64,
    key_lengths: Vec<usize>,
    version: Option<FormatVersion>,
}

/// Handler для фильтрации записей по ключу.
///
/// Загружает в память только записи, удовлетворяющие предикату.
pub struct FilterHandler<F>
where
    F: Fn(&Sds) -> bool,
{
    predicate: F,
    items: Vec<(Sds, Value)>,
}

/// Handler с callback ф-ей для каждой записи.
///
/// Позволяет обрабатывать записи без создания custom handler.
pub struct CallbackHandler<F>
where
    F: FnMut(Sds, Value) -> ZumicResult<()>,
{
    callback: F,
}

/// Handler для записи в другой дамп с трансформацией.
pub struct TransformHandler<W, F>
where
    W: Write,
    F: Fn(&Sds, &Value) -> Option<(Sds, Value)>,
{
    writer: W,
    transform: F,
    count: u64,
}

impl CollectHandler {
    /// Создаёт новый CollectHandler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Возвращает собаранные записи.
    pub fn items(&self) -> &[(Sds, Value)] {
        &self.items
    }

    /// Забирает собранные записи.
    pub fn into_items(self) -> Vec<(Sds, Value)> {
        self.items
    }
}

impl CountHandler {
    /// Создаёт новый CountHandler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Возвращает общее кол-во записей.
    pub fn total_entries(&self) -> u64 {
        self.total_entries
    }

    /// Вовзаращает среднюю дилну ключа.
    pub fn avg_key_length(&self) -> f64 {
        if self.key_lengths.is_empty() {
            0.0
        } else {
            self.key_lengths.iter().sum::<usize>() as f64 / self.key_lengths.len() as f64
        }
    }

    /// Возвращает версию дампа.
    pub fn version(&self) -> Option<FormatVersion> {
        self.version
    }
}

impl<R: Read> StreamingParser<R> {
    /// Создаёт новый парсер из Reader.
    pub fn new(reader: R) -> ZumicResult<Self> {
        Self::new_with_version(reader, FormatVersion::current())
    }

    /// Создаёт парсер с явно указанной версией читателя.
    pub fn new_with_version(
        reader: R,
        reader_version: FormatVersion,
    ) -> ZumicResult<Self> {
        Ok(Self {
            reader: BufReader::with_capacity(64 * 1024, reader),
            version: None,
            stats: ParseStats::default(),
            reader_version,
        })
    }

    /// Парсит дамп, вызывая handler для каждого события.
    pub fn parse<H: ParseHandler>(
        &mut self,
        handler: &mut H,
    ) -> ZumicResult<()> {
        // Читаем и валидируем заголовок
        let version = self.read_and_validate_header()?;
        self.version = Some(version);
        self.stats.version = Some(version);

        // Проверяем совметсимость
        let compatibility = VersionUtils::validate_compatibility(self.reader_version, version)
            .map_err(ZdbError::from)?;

        // Отправляем событие Header
        handler.handle_event(ParseEvent::Header {
            version,
            compatibility: compatibility.clone(),
        })?;

        // Читаем записи до EOF
        loop {
            let offset = self.stats.bytes_read;

            match self.read_next_entry(version, offset) {
                Ok(Some((key, value))) => {
                    self.stats.records_parsed += 1;

                    // Отправляем событие Entry
                    if let Err(e) = handler.handle_event(ParseEvent::Entry {
                        key: key.clone(),
                        value,
                    }) {
                        // Handler вернул ошибку
                        let error_event = ParseEvent::Error {
                            error: e.to_string(),
                            key: Some(key),
                            offset: Some(offset),
                            recoverable: false,
                        };

                        handler.handle_event(error_event)?;
                        return Err(e);
                    }
                }
                Ok(None) => {
                    // EOF достигнут
                    break;
                }
                Err(e) => {
                    self.stats.errors_count += 1;

                    // Проверяем, является ли ошибка recoverable
                    let recoverable = e
                        .downcast_ref::<ZdbError>()
                        .map(|z| z.is_recoverable())
                        .unwrap_or(false);

                    // Создаём событие ошибки
                    let error_event = ParseEvent::Error {
                        error: e.to_string(),
                        key: None,
                        offset: Some(offset),
                        recoverable,
                    };

                    handler.handle_event(error_event.clone())?;

                    // Решаем продолжать ли
                    if !handler.should_continue_on_error() {
                        return Err(e);
                    }
                    self.stats.skipped_records += 1;
                }
            }
        }

        // Отправляем событие End
        handler.handle_event(ParseEvent::End)?;

        // Финализация
        handler.finalize()?;

        Ok(())
    }

    /// Возвращает статистику парсинга.
    pub fn stats(&self) -> &ParseStats {
        &self.stats
    }

    /// Возвращает версию дампа (если заголовок был прочитан).
    pub fn version(&self) -> Option<FormatVersion> {
        self.version
    }

    /// Потребляет парсер и возвращает внутренний BufReader<R>.
    pub fn into_inner(self) -> BufReader<R> {
        self.reader
    }

    fn read_and_validate_header(&mut self) -> ZumicResult<FormatVersion> {
        let start_offset = self.stats.bytes_read;

        // Читаем magic number
        let mut magic = [0u8; 3];
        self.reader
            .read_exact(&mut magic)
            .map_err(|_| ZdbError::UnexpectedEof {
                context: "reading magic number".to_string(),
                offset: Some(start_offset),
                key: None,
                expected_bytes: Some(3),
                got_bytes: Some(0),
            })?;
        self.stats.bytes_read += 3;

        ensure!(
            &magic == FILE_MAGIC,
            ZdbError::InvalidMagic {
                expected: *FILE_MAGIC,
                got: magic
            }
        );

        // Читаем версию
        let mut version_bytes = [0u8; 1];
        self.reader
            .read_exact(&mut version_bytes)
            .map_err(|_| ZdbError::UnexpectedEof {
                context: "reading version byte".to_string(),
                offset: Some(start_offset),
                key: None,
                expected_bytes: Some(1),
                got_bytes: Some(0),
            })?;
        self.stats.bytes_read += 1;

        let version = FormatVersion::try_from(version_bytes[0]).map_err(ZdbError::from)?;

        Ok(version)
    }

    fn read_next_entry(
        &mut self,
        version: FormatVersion,
        offset: u64,
    ) -> ZumicResult<Option<(Sds, Value)>> {
        // Пытаемся прочитать первый байт
        let mut peek = [0u8; 1];
        match self.reader.read_exact(&mut peek) {
            Ok(_) => {
                // успешно прочли байт - продолжаем
                self.stats.bytes_read += 1;
            }
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                // Если до этого не было разобрано ни одной записи — считаем это
                // корректным "честным" EOF (например, пустой дамп).
                // Но если мы уже успешно разобрали хотя бы одну запись,
                // то внезапный EOF — признак усечения файла => ошибка.
                if self.stats.records_parsed == 0 {
                    return Ok(None);
                }
                return Err(ZdbError::UnexpectedEof {
                    context: "expecting next entry".to_string(),
                    offset: Some(offset),
                    key: None,
                    expected_bytes: Some(1),
                    got_bytes: Some(0),
                }
                .into());
            }
            Err(e) => return Err(StackError::from(e)),
        }
        self.stats.bytes_read += 1;

        // Проверяем на TAG_EOF
        if peek[0] == TAG_EOF {
            return Ok(None);
        }

        // Это первый байт длины ключа
        let mut len_buf = [0u8; 4];
        len_buf[0] = peek[0];
        self.reader
            .read_exact(&mut len_buf[1..])
            .map_err(|_| ZdbError::UnexpectedEof {
                context: "reading key length".to_string(),
                offset: Some(offset),
                key: None,
                expected_bytes: Some(4),
                got_bytes: Some(1),
            })?;
        self.stats.bytes_read += 3;

        let key_len = u32::from_be_bytes(len_buf) as usize;

        const MAX_KEY_SIZE: usize = 512 * 1024 * 1024; // 512МБ
        ensure!(
            key_len <= MAX_KEY_SIZE,
            ZdbError::SizeLimit {
                what: "key".to_string(),
                size: key_len as u64,
                limit: MAX_KEY_SIZE as u64,
                offset: Some(offset),
                key: None
            }
        );

        // Читаем ключ
        let mut key_bytes = vec![0u8; key_len];
        self.reader
            .read_exact(&mut key_bytes)
            .map_err(|_| ZdbError::UnexpectedEof {
                context: "reading key bytes".to_string(),
                offset: Some(offset + 4),
                key: None,
                expected_bytes: Some(key_len as u64),
                got_bytes: Some(0),
            })?;
        self.stats.bytes_read += key_len as u64;

        let key = Sds::from_vec(key_bytes);
        let key_str = String::from_utf8_lossy(key.as_bytes()).to_string();

        let value_offset = offset + 4 + key_len as u64;
        let value =
            read_value_with_version(&mut self.reader, version, Some(&key_str), value_offset)?;

        Ok(Some((key, value)))
    }
}

impl<R: Read> Crc32Read<R> {
    /// Создаёт новый Crc32Read.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: crc32fast::Hasher::new(),
            bytes_read: 0,
        }
    }

    /// Возвращает (inner, crc) - потребляет self.
    pub fn into_inner_and_finalize(self) -> (R, u32) {
        let crc = self.hasher.finalize();
        (self.inner, crc)
    }

    /// Возвращает текущее кол-во прочитанных байт.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Текущий (не финализированный) CRC.
    pub fn current_crc(&self) -> u32 {
        self.hasher.clone().finalize()
    }
}

impl ParseHandler for CollectHandler {
    fn handle_event(
        &mut self,
        event: ParseEvent,
    ) -> ZumicResult<()> {
        if let ParseEvent::Entry { key, value } = event {
            self.items.push((key, value));
        }
        Ok(())
    }
}

impl<F> FilterHandler<F>
where
    F: Fn(&Sds) -> bool,
{
    /// Создаёт handler с предикатом фильтрации.
    pub fn new(predicate: F) -> Self {
        Self {
            predicate,
            items: Vec::new(),
        }
    }

    /// Возвращает отфильтрованные записи.
    pub fn items(&self) -> &[(Sds, Value)] {
        &self.items
    }

    /// Забирает отфильтрованные записи.
    pub fn into_items(self) -> Vec<(Sds, Value)> {
        self.items
    }
}

impl<F> ParseHandler for FilterHandler<F>
where
    F: Fn(&Sds) -> bool,
{
    fn handle_event(
        &mut self,
        event: ParseEvent,
    ) -> ZumicResult<()> {
        if let ParseEvent::Entry { key, value } = event {
            if (self.predicate)(&key) {
                self.items.push((key, value));
            }
        }
        Ok(())
    }
}

impl ParseHandler for CountHandler {
    fn handle_event(
        &mut self,
        event: ParseEvent,
    ) -> ZumicResult<()> {
        match event {
            ParseEvent::Header { version, .. } => {
                self.version = Some(version);
            }
            ParseEvent::Entry { key, .. } => {
                self.total_entries += 1;
                self.key_lengths.push(key.len());
            }
            _ => {}
        }
        Ok(())
    }
}

impl<F> CallbackHandler<F>
where
    F: FnMut(Sds, Value) -> ZumicResult<()>,
{
    /// Создаёт handler с callback ф-ей.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F> ParseHandler for CallbackHandler<F>
where
    F: FnMut(Sds, Value) -> ZumicResult<()>,
{
    fn handle_event(
        &mut self,
        event: ParseEvent,
    ) -> ZumicResult<()> {
        if let ParseEvent::Entry { key, value } = event {
            (self.callback)(key, value)?;
        }
        Ok(())
    }
}

impl<W, F> TransformHandler<W, F>
where
    W: Write,
    F: Fn(&Sds, &Value) -> Option<(Sds, Value)>,
{
    /// Создаёт handler для трансформации и записи.
    pub fn new(
        writer: W,
        transform: F,
    ) -> Self {
        Self {
            writer,
            transform,
            count: 0,
        }
    }

    /// Возвращает кол-во записанных записей.
    pub fn count(&self) -> u64 {
        self.count
    }
}

impl<W, F> ParseHandler for TransformHandler<W, F>
where
    W: Write,
    F: Fn(&Sds, &Value) -> Option<(Sds, Value)>,
{
    fn handle_event(
        &mut self,
        event: ParseEvent,
    ) -> ZumicResult<()> {
        match event {
            ParseEvent::Header { version, .. } => {
                // Записываем заголовок
                self.writer
                    .write_all(FILE_MAGIC)
                    .context("Failed to write magic")?;
                self.writer
                    .write_all(&[version as u8])
                    .context("Failed to write version")?;
            }
            ParseEvent::Entry { key, value } => {
                // Применяем трансформацию
                if let Some((new_key, new_value)) = (self.transform)(&key, &value) {
                    let kb = new_key.as_bytes();
                    self.writer
                        .write_u32::<BigEndian>(kb.len() as u32)
                        .context("Failed to write key length")?;
                    self.writer.write_all(kb).context("Failed to write key")?;
                    write_value(&mut self.writer, &new_value).context("Failed to write value")?;
                    self.count += 1;
                }
            }
            ParseEvent::End => {
                // Записываем EOF
                self.writer.write_all(&[TAG_EOF])?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl<R: Read> Read for Crc32Read<R> {
    fn read(
        &mut self,
        buf: &mut [u8],
    ) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.hasher.update(&buf[..n]);
            self.bytes_read += n as u64;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::engine::write_stream;

    /// Тест проверяет, что CollectHandler собирает все записи из дампа в Vec.
    #[test]
    fn test_collect_handler() {
        let items = vec![
            (Sds::from_str("key1"), Value::Int(1)),
            (Sds::from_str("key2"), Value::Str(Sds::from_str("value2"))),
        ];

        let mut buf = Vec::new();
        write_stream(&mut buf, items.clone().into_iter()).unwrap();

        let mut parser = StreamingParser::new(Cursor::new(buf)).unwrap();
        let mut handler = CollectHandler::new();
        parser.parse(&mut handler).unwrap();

        assert_eq!(handler.items(), &items);
    }

    /// Тест проверяет, что FilterHandler фильтрует записи по предикату и
    /// возвращает только подходящие ключи.
    #[test]
    fn test_filter_handler() {
        let items = vec![
            (Sds::from_str("user:1"), Value::Int(1)),
            (Sds::from_str("post:1"), Value::Int(2)),
            (Sds::from_str("user:2"), Value::Int(3)),
        ];

        let mut buf = Vec::new();
        write_stream(&mut buf, items.into_iter()).unwrap();

        let mut parser = StreamingParser::new(Cursor::new(buf)).unwrap();
        let mut handler = FilterHandler::new(|key| key.starts_with(b"user:"));
        parser.parse(&mut handler).unwrap();

        assert_eq!(handler.items().len(), 2);
        assert!(handler.items()[0].0.starts_with(b"user:"));
        assert!(handler.items()[1].0.starts_with(b"user:"));
    }

    /// Тест проверяет, что CountHandler корректно считает общее число записей и
    /// вычисляет среднюю длину ключа.
    #[test]
    fn test_count_handler() {
        let items = vec![
            (Sds::from_str("a"), Value::Int(1)),
            (Sds::from_str("bb"), Value::Int(2)),
            (Sds::from_str("ccc"), Value::Int(3)),
        ];

        let mut buf = Vec::new();
        write_stream(&mut buf, items.into_iter()).unwrap();

        let mut parser = StreamingParser::new(Cursor::new(buf)).unwrap();
        let mut handler = CountHandler::new();
        parser.parse(&mut handler).unwrap();

        assert_eq!(handler.total_entries(), 3);
        assert_eq!(handler.avg_key_length(), 2.0);
    }

    /// Тест проверяет, что CallbackHandler вызывает переданную функцию для
    /// каждой записи.
    #[test]
    fn test_callback_handler() {
        let items = vec![(Sds::from_str("key"), Value::Int(42))];

        let mut buf = Vec::new();
        write_stream(&mut buf, items.into_iter()).unwrap();

        let mut parser = StreamingParser::new(Cursor::new(buf)).unwrap();
        let mut collected = Vec::new();

        let mut handler = CallbackHandler::new(|key, value| {
            collected.push((key, value));
            Ok(())
        });

        parser.parse(&mut handler).unwrap();

        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].0, Sds::from_str("key"));
    }

    /// Тест проверяет, что parser обновляет статистику: количество записей,
    /// число ошибок и количество прочитанных байт.
    #[test]
    fn test_parse_stats() {
        let items = vec![
            (Sds::from_str("key1"), Value::Int(1)),
            (Sds::from_str("key2"), Value::Int(2)),
        ];

        let mut buf = Vec::new();
        write_stream(&mut buf, items.into_iter()).unwrap();

        let mut parser = StreamingParser::new(Cursor::new(buf)).unwrap();
        let mut handler = CollectHandler::new();
        parser.parse(&mut handler).unwrap();

        let stats = parser.stats();
        assert_eq!(stats.records_parsed, 2);
        assert_eq!(stats.errors_count, 0);
        assert!(stats.bytes_read > 0);
    }

    /// Тест проверяет те же поля статистики парсера (дублирующий/повторный тест
    /// для контроля стабильности).
    #[test]
    fn test_parser_stats() {
        let items = vec![
            (Sds::from_str("key1"), Value::Int(1)),
            (Sds::from_str("key2"), Value::Int(2)),
        ];

        let mut buf = Vec::new();
        write_stream(&mut buf, items.into_iter()).unwrap();

        let mut parser = StreamingParser::new(Cursor::new(buf)).unwrap();
        let mut handler = CollectHandler::new();
        parser.parse(&mut handler).unwrap();

        let stats = parser.stats();
        assert_eq!(stats.records_parsed, 2);
        assert_eq!(stats.errors_count, 0);
        assert!(stats.bytes_read > 0);
    }

    /// Тест проверяет, что парсер корректно обрабатывает пустой дамп и не
    /// возвращает записей.
    #[test]
    fn test_empty_dump() {
        let mut buf = Vec::new();
        write_stream(&mut buf, Vec::<(Sds, Value)>::new().into_iter()).unwrap();

        let mut parser = StreamingParser::new(Cursor::new(buf)).unwrap();
        let mut handler = CollectHandler::new();
        parser.parse(&mut handler).unwrap();

        assert_eq!(handler.items().len(), 0);
    }
}
