//! Определение тегов для бинарного формата значений `Value`.
//!
//! Каждый тип данных помечается однобайтовым значением.
//! Используется в модулях `decode` и `encode`.

/// Строка (Sds)
pub const TAG_STR: u8 = 0x01;
/// Целое число (i64)
pub const TAG_INT: u8 = 0x02;
/// Число с плавающей точкой (f64)
pub const TAG_FLOAT: u8 = 0x03;
/// Логическое значение (bool)
pub const TAG_BOOL: u8 = 0x0B;
/// Null
pub const TAG_NULL: u8 = 0x04;
/// Список строк
pub const TAG_LIST: u8 = 0x05;
/// Хэш-таблица (map<Sds, Sds>)
pub const TAG_HASH: u8 = 0x06;
/// Упорядоченное множество (ZSet)
pub const TAG_ZSET: u8 = 0x07;
/// Множество (Set<Sds>)
pub const TAG_SET: u8 = 0x08;
/// HyperLogLog
pub const TAG_HLL: u8 = 0x09;
/// Поток (Stream)
pub const TAG_SSTREAM: u8 = 0x0A;
/// Сжатый блок данных (zstd)
pub const TAG_COMPRESSED: u8 = 0x0C;
/// Маркер конца потока (EOF) в streaming-формате дампа.
pub const TAG_EOF: u8 = 0xFF;
/// Общий массив произвольных значений (`Value::Array`)
pub const TAG_ARRAY: u8 = 0x0D;
/// Битовый массив (`Value::Bitmap`)
pub const TAG_BITMAP: u8 = 0x0E;
