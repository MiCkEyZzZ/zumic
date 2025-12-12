//! Variable-length integer encoding (LEB128-style).
//!
//! Экономит место для маленьких чисел:
//! - 0-127: 1 байт
//! - 128-16383: 2 байта
//! - 16384-2097151: 3 байта
//! - до u32::MAX: 5 байт максимум

use std::io::{Read, Write};

use zumic_error::{ResultExt, ZdbError, ZumicResult};

/// Максимальное кол-во байт для u32 в varint encoding (5 байт)
pub const MAX_VARINT_LEN: usize = 5;

/// Записывает u32 в varint формате.
///
/// # Формат
/// - Каждый байт: 7 бит данных + 1 бит continuation
/// - MSB=1: есть ещё байты
/// - MSB=0: последний байт
///
/// # Examples
/// ```
/// use std::io::Cursor;
///
/// use zumic::engine::varint::write_varint;
///
/// let mut buf = Vec::new();
/// write_varint(&mut buf, 127).unwrap();
/// assert_eq!(buf, vec![0x7F]); // 1 байт
///
/// let mut buf = Vec::new();
/// write_varint(&mut buf, 128).unwrap();
/// assert_eq!(buf, vec![0x80, 0x01]); // 2 байта
/// ```
pub fn write_varint<W: Write>(
    w: &mut W,
    mut value: u32,
) -> ZumicResult<usize> {
    let mut bytes_written = 0;

    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;

        if value != 0 {
            byte |= 0x80; // Continuation bit
        }

        w.write_all(&[byte])
            .context("Failed to write varint byte")?;
        bytes_written += 1;

        if value == 0 {
            break;
        }
    }

    Ok(bytes_written)
}

/// Читает u32 из varint формата.
///
/// # Errors
/// - `UnexpectedEof` если файл кончился раньше времени
/// - `InvalidData` если varint слишком длинный (>5 байт)
///
/// # Examples
/// ```
/// use std::io::Cursor;
///
/// use zumic::engine::varint::read_varint;
///
/// let data = vec![0x7F]; // 127
/// let mut cursor = Cursor::new(data);
/// assert_eq!(read_varint(&mut cursor).unwrap(), 127);
///
/// let data = vec![0x80, 0x01]; // 128
/// let mut cursor = Cursor::new(data);
/// assert_eq!(read_varint(&mut cursor).unwrap(), 128);
/// ```
pub fn read_varint<R: Read>(r: &mut R) -> ZumicResult<u32> {
    let mut result: u32 = 0;
    let mut shift = 0;

    for i in 0..MAX_VARINT_LEN {
        let mut buf = [0u8; 1];
        r.read_exact(&mut buf)
            .context("Failed to read varint byte")?;

        let byte = buf[0];
        result |= ((byte & 0x7F) as u32) << shift;

        if byte & 0x80 == 0 {
            return Ok(result);
        }

        shift += 7;

        // NOTE: Защита varint не должен быть длиннее 5 байт для u32
        if i == MAX_VARINT_LEN - 1 {
            return Err(ZdbError::ParseError {
                structure: "varint".to_string(),
                reason: format!("Varint too long (>{MAX_VARINT_LEN} bytes), possible corruption",),
                offset: None,
                key: None,
            }
            .into());
        }
    }

    unreachable!()
}

/// Вычисляет размер varint для числа (без записи).
///
/// Полезно для предварительного расчёта размера буфера.
pub fn varint_size(mut value: u32) -> usize {
    if value == 0 {
        return 1;
    }

    let mut size = 0;
    while value != 0 {
        value >>= 7;
        size += 1;
    }
    size
}

/// Проверяет, выгодно ли использовать varint (экономия места).
///
/// Varint выгоден для значений <16384 (2 байта vs 4 байта fixed).
pub fn varint_is_efficient(value: u32) -> bool {
    varint_size(value) < 4
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn test_varint_size() {
        assert_eq!(varint_size(0), 1);
        assert_eq!(varint_size(127), 1);
        assert_eq!(varint_size(128), 2);
        assert_eq!(varint_size(16383), 2);
        assert_eq!(varint_size(16384), 3);
        assert_eq!(varint_size(u32::MAX), 5);
    }

    #[test]
    fn test_varint_roundtrip() {
        let test_cases = vec![
            0,
            1,
            127,
            128,
            255,
            256,
            16383,
            16384,
            65535,
            1_000_000,
            u32::MAX,
        ];

        for &value in &test_cases {
            let mut buf = Vec::new();
            let written = write_varint(&mut buf, value).unwrap();

            let mut cursor = Cursor::new(&buf);
            let decoded = read_varint(&mut cursor).unwrap();

            assert_eq!(
                decoded, value,
                "Roundtrip failed for {value}: got {decoded}",
            );
            assert_eq!(written, buf.len(), "Size mismatch for {value}");
            assert_eq!(
                written,
                varint_size(value),
                "Size calculation wrong for {value}"
            );
        }
    }

    #[test]
    fn test_varint_boundaries() {
        // 1 byte: 0-127
        assert_eq!(varint_size(127), 1);
        assert_eq!(varint_size(128), 2);

        // 2 bytes: 128-16383
        assert_eq!(varint_size(16383), 2);
        assert_eq!(varint_size(16384), 3);

        // 3 bytes: 16384-2097151
        assert_eq!(varint_size(2_097_151), 3);
        assert_eq!(varint_size(2_097_152), 4);

        // 4 bytes: 2097152-268435455
        assert_eq!(varint_size(268_435_455), 4);
        assert_eq!(varint_size(268_435_456), 5);

        // 5 bytes: 268435456-u32::MAX
        assert_eq!(varint_size(u32::MAX), 5);
    }

    #[test]
    fn test_varint_efficiency() {
        // 1 байт - выгодно
        assert!(varint_is_efficient(0));
        assert!(varint_is_efficient(127));

        // 2 байта - выгодно
        assert!(varint_is_efficient(128));
        assert!(varint_is_efficient(16383));

        // 3 байта - выгодно! (3 < 4 байта fixed)
        assert!(varint_is_efficient(16384));
        assert!(varint_is_efficient(2_097_151));

        // 4 байта - НЕ выгодно (4 == 4 байта fixed)
        assert!(!varint_is_efficient(2_097_152));
        assert!(!varint_is_efficient(268_435_455));

        // 5 байт - НЕ выгодно (5 > 4 байта fixed)
        assert!(!varint_is_efficient(268_435_456));
        assert!(!varint_is_efficient(u32::MAX));
    }

    #[test]
    fn test_varint_invalid_long() {
        // 6 байт с continuation bits (невалидно)
        let bad_data = vec![0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        let mut cursor = Cursor::new(bad_data);
        let err = read_varint(&mut cursor).unwrap_err();

        let err_msg = err.to_string();
        assert!(
            err_msg.contains("too long") || err_msg.contains("corruption"),
            "Expected 'too long' error, got: {err_msg}"
        );
    }

    #[test]
    fn test_varint_unexpected_eof() {
        // Неполный varint (continuation bit установлен, но данных нет)
        let incomplete = vec![0x80]; // ожидается ещё байт
        let mut cursor = Cursor::new(incomplete);
        let err = read_varint(&mut cursor).unwrap_err();

        let err_msg = err.to_string();
        assert!(
            err_msg.contains("Failed") || err_msg.contains("EOF"),
            "Expected EOF error, got: {err_msg}"
        );
    }

    #[test]
    fn test_varint_zero() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 0).unwrap();
        assert_eq!(buf, vec![0x00]);

        let mut cursor = Cursor::new(buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), 0);
    }

    #[test]
    fn test_varint_max_u32() {
        let mut buf = Vec::new();
        write_varint(&mut buf, u32::MAX).unwrap();
        assert_eq!(buf.len(), 5);

        let mut cursor = Cursor::new(buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), u32::MAX);
    }

    #[test]
    fn test_varint_size_calculation_matches_encoding() {
        for value in [0, 1, 127, 128, 16383, 16384, u32::MAX] {
            let mut buf = Vec::new();
            write_varint(&mut buf, value).unwrap();
            assert_eq!(buf.len(), varint_size(value), "Size mismatch for {value}");
        }
    }

    #[test]
    fn test_known_encodings() {
        // 300 => 0xAC, 0x02
        let mut buf = Vec::new();
        write_varint(&mut buf, 300).unwrap();
        assert_eq!(buf, vec![0xAC, 0x02]);

        // u32::MAX => 0xFF,0xFF,0xFF,0xFF,0x0F
        let mut buf = Vec::new();
        write_varint(&mut buf, u32::MAX).unwrap();
        assert_eq!(buf, vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    }

    #[test]
    fn test_read_leaves_extra_bytes() {
        // varint(300) + extra 0x42
        let data = vec![0xAC, 0x02, 0x42];
        let mut cursor = Cursor::new(data);
        let v = read_varint(&mut cursor).unwrap();
        assert_eq!(v, 300);

        // следующий байт должен быть 0x42
        let mut next = [0u8; 1];
        cursor.read_exact(&mut next).unwrap();
        assert_eq!(next[0], 0x42);
    }

    #[test]
    fn test_multiple_incomplete_varints() {
        let cases = vec![
            vec![0x80],                   // ждёт ещё 1 байт
            vec![0x80, 0x80],             // ждёт ещё
            vec![0x80, 0x80, 0x80, 0x80], // ждёт ещё
        ];

        for case in cases {
            let mut cursor = Cursor::new(case);
            let err = read_varint(&mut cursor).unwrap_err();
            let s = err.to_string();
            assert!(
                s.contains("Failed") || s.contains("EOF") || s.contains("unexpected"),
                "expected EOF-like error, got: {s}"
            );
        }
    }
}
