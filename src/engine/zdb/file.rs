/// «Магическое» начало файла: ASCII-буквы «ZDB».
pub const FILE_MAGIC: &[u8; 3] = b"ZDB";

/// Текущая версия формата дампа.
pub const DUMP_VERSION: u8 = 1;
