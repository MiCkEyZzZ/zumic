/// «Магическое» начало файла: ASCII-буквы «ZDB».
pub const FILE_MAGIC: &[u8; 3] = b"ZDB";

/// Поддерживаемые версии формата дампа ZDB.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVersion {
    V1 = 1,
    // В будущем: V2 = 2, V3 = 3 и т.д.
}

impl TryFrom<u8> for FormatVersion {
    type Error = std::io::Error;
    fn try_from(value: u8) -> std::io::Result<Self> {
        match value {
            1 => Ok(FormatVersion::V1),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unsupported version of the ZDB dump: {other}"),
            )),
        }
    }
}

/// Текущая версия формата дампа, как число (для совместимости).
pub const DUMP_VERSION: u8 = FormatVersion::V1 as u8;
