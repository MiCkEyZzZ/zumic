use std::{fmt, num::IntErrorKind};

use crate::Sds;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdsNumericError {
    Empty,
    InvalidInteger,
    Overflow,
    InvalidUtf8,
}

/// Стековый буфер дляформатирования `f64` без heap-аллокации.
struct StackFmtBuf<const N: usize> {
    buf: [u8; 32],
    len: usize,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<const N: usize> StackFmtBuf<N> {
    #[inline]
    fn new() -> Self {
        Self {
            buf: [0u8; 32],
            len: 0,
        }
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl<const N: usize> std::fmt::Write for StackFmtBuf<N> {
    #[inline]
    fn write_str(
        &mut self,
        s: &str,
    ) -> fmt::Result {
        let bytes = s.as_bytes();
        let end = self.len + bytes.len();

        if end > N {
            return Err(std::fmt::Error);
        }

        self.buf[self.len..end].copy_from_slice(bytes);
        self.len = end;

        Ok(())
    }
}

impl Sds {
    /// Создаёт `Sds` из `i64` без использования `format!` и без heap-аллокации.
    pub fn from_i64(n: i64) -> Self {
        let mut buf = [0u8; 20];
        let len = write_i64(n, &mut buf);

        Self::from_bytes(&buf[..len])
    }

    /// Создаёт `Sds` из `u64` без использования `format!` и без heap-аллокации.
    pub fn from_u64(n: u64) -> Self {
        let mut buf = [0u8; 20];
        let len = write_u64(n, &mut buf);

        Self::from_bytes(&buf[..len])
    }

    /// Создаёт `Sds` из `f64` без heap-аллокации.
    pub fn from_f64(n: f64) -> Self {
        use std::fmt::Write as FmtWrite;

        let mut buf = StackFmtBuf::<32>::new();
        let _ = write!(buf, "{n}");

        Self::from_bytes(buf.as_bytes())
    }

    /// Парсит строку как `i64`.
    pub fn to_i64(&self) -> Result<i64, SdsNumericError> {
        let bytes = self.as_slice();

        if bytes.is_empty() {
            return Err(SdsNumericError::Empty);
        }

        // from_utf8 на ASCII-цифрах - константная работа без копирования.
        let s = std::str::from_utf8(bytes).map_err(|_| SdsNumericError::InvalidUtf8)?;

        s.parse::<i64>().map_err(|e| match e.kind() {
            IntErrorKind::PosOverflow | IntErrorKind::NegOverflow => SdsNumericError::Overflow,
            IntErrorKind::Empty => SdsNumericError::Empty,
            _ => SdsNumericError::InvalidInteger,
        })
    }

    /// Парсит строку как `u64`.
    pub fn to_u64(&self) -> Result<u64, SdsNumericError> {
        let bytes = self.as_slice();

        if bytes.is_empty() {
            return Err(SdsNumericError::Empty);
        }

        let s = std::str::from_utf8(bytes).map_err(|_| SdsNumericError::InvalidUtf8)?;

        s.parse::<u64>().map_err(|e| match e.kind() {
            IntErrorKind::PosOverflow => SdsNumericError::Overflow,
            IntErrorKind::Empty => SdsNumericError::Empty,
            _ => SdsNumericError::InvalidInteger,
        })
    }

    /// Парсит строку как `f64`.
    pub fn to_f64(&self) -> Result<f64, SdsNumericError> {
        let bytes = self.as_slice();

        if bytes.is_empty() {
            return Err(SdsNumericError::Empty);
        }

        let s = std::str::from_utf8(bytes).map_err(|_| SdsNumericError::InvalidUtf8)?;

        s.parse::<f64>()
            .map_err(|_| SdsNumericError::InvalidInteger)
    }

    /// Проверяет, является ли строка валидным целым числом.
    #[inline]
    pub fn is_integer(&self) -> bool {
        let bytes = self.as_slice();

        if bytes.is_empty() {
            return false;
        }

        // Пропускаем опциональный знак
        let digits = match bytes[0] {
            b'-' | b'+' => &bytes[1..],
            _ => bytes,
        };

        !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Внутренние функции
////////////////////////////////////////////////////////////////////////////////

/// Записывает десятичное представление `n` (i64) в `buf[..20]` начиная
/// с индекса 0. Возвращает количество записанных байт.
#[inline]
fn write_i64(
    n: i64,
    buf: &mut [u8; 20],
) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let negative = n < 0;
    let mut magnitude = n.unsigned_abs();
    let mut pos = 20usize;

    while magnitude > 0 {
        pos -= 1;
        buf[pos] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
    }

    if negative {
        pos -= 1;
        buf[pos] = b'-';
    }

    let len = 20 - pos;

    buf.copy_within(pos..20, 0);

    len
}

/// Записывает десятичное представление `n` (u64) в `buf[..20]`.
/// Возвращает количество записанных байт.
#[inline]
fn write_u64(
    mut n: u64,
    buf: &mut [u8; 20],
) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut pos = 20usize;

    while n > 0 {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    let len = 20 - pos;

    buf.copy_within(pos..20, 0);

    len
}

/// Парсит байты как `i64` без промежуточного `String`.
#[allow(dead_code)]
fn parse_i64(bytes: &[u8]) -> Result<i64, SdsNumericError> {
    if bytes.is_empty() {
        return Err(SdsNumericError::Empty);
    }

    let (neg, digs) = match bytes[0] {
        b'-' => (true, &bytes[1..]),
        b'+' => (false, &bytes[1..]),
        _ => (false, bytes),
    };

    if digs.is_empty() {
        return Err(SdsNumericError::InvalidInteger);
    }

    let mut result: u64 = 0;

    for &b in digs {
        if !b.is_ascii_digit() {
            return Err(SdsNumericError::InvalidInteger);
        }

        result = result
            .checked_mul(10)
            .and_then(|r| r.checked_add((b - b'0') as u64))
            .ok_or(SdsNumericError::Overflow)?;
    }

    if neg {
        if result > i64::MAX as u64 + 1 {
            return Err(SdsNumericError::Overflow);
        }

        Ok(result.wrapping_neg() as i64)
    } else {
        if result > i64::MAX as u64 {
            return Err(SdsNumericError::Overflow);
        }

        Ok(result as i64)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для SdsNumericError
////////////////////////////////////////////////////////////////////////////////

impl fmt::Display for SdsNumericError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "ERR value is empty"),
            Self::InvalidInteger => {
                write!(f, "ERR value is not an integer or out of range")
            }
            Self::Overflow => write!(f, "ERR value is out of range"),
            Self::InvalidUtf8 => write!(f, "ERR value contains invalid bytes"),
        }
    }
}

impl std::error::Error for SdsNumericError {}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_i64_zero() {
        let s = Sds::from_i64(0);

        assert_eq!(s.as_str().unwrap(), "0");
        assert!(s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_i64_positive() {
        assert_eq!(Sds::from_i64(1).as_str().unwrap(), "1");
        assert_eq!(Sds::from_i64(42).as_str().unwrap(), "42");
        assert_eq!(Sds::from_i64(9999).as_str().unwrap(), "9999");
        assert_eq!(Sds::from_i64(1_000_000).as_str().unwrap(), "1000000");
    }

    #[test]
    fn test_from_i64_negative() {
        assert_eq!(Sds::from_i64(-1).as_str().unwrap(), "-1");
        assert_eq!(Sds::from_i64(-42).as_str().unwrap(), "-42");
        assert_eq!(Sds::from_i64(-9999).as_str().unwrap(), "-9999");
    }

    #[test]
    fn test_from_i64_max() {
        let s = Sds::from_i64(i64::MAX);

        assert_eq!(s.as_str().unwrap(), "9223372036854775807");
        assert!(s.is_inline(), "i64::MAX (19 chars) must be inline");
    }

    #[test]
    fn test_from_i64_min() {
        let s = Sds::from_i64(i64::MIN);

        assert_eq!(s.as_str().unwrap(), "-9223372036854775808");
        assert!(s.is_inline(), "i64::MIN (20 chars) must be inline");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_i64_always_inline() {
        for n in [0i64, 1, -1, 100, -100, i64::MAX, i64::MIN] {
            let s = Sds::from_i64(n);

            assert!(s.is_inline(), "from_i64({n}) must be inline");
        }
    }

    #[test]
    fn test_from_i64_roundtrip() {
        for n in [0i64, 1, -1, 42, -42, 1000, -1000, i64::MAX, i64::MIN] {
            let s = Sds::from_i64(n);
            let back = s
                .to_i64()
                .unwrap_or_else(|e| panic!("roundtrip failed for {n}: {e}"));

            assert_eq!(n, back);
        }
    }

    #[test]
    fn test_u64_zero() {
        let s = Sds::from_u64(0);

        assert_eq!(s.as_str().unwrap(), "0");
        assert!(s.is_inline());
    }

    #[test]
    fn test_from_u64_max() {
        let s = Sds::from_u64(u64::MAX);

        assert_eq!(s.as_str().unwrap(), "18446744073709551615");
        assert!(s.is_inline(), "u64::MAX (20 chars) must be inline");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_u64_always_inline() {
        for n in [0u64, 1, 42, 255, 65535, u64::MAX] {
            let s = Sds::from_u64(n);

            assert!(s.is_inline(), "from_u64({n}) must be inline");
        }
    }
}
