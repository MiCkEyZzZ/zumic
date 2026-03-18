use std::{fmt, num::IntErrorKind};

use crate::Sds;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdsNumericError {
    Empty,
    InvalidInteger,
    InvalidFloat,
    Overflow,
    InvalidUtf8,
}

/// Стековый буфер дляформатирования `f64` без heap-аллокации.
struct StackFmtBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<const N: usize> StackFmtBuf<N> {
    #[inline]
    fn new() -> Self {
        Self {
            buf: [0u8; N],
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
        let slice = write_i64(n, &mut buf);

        Self::from_bytes(slice)
    }

    /// Создаёт `Sds` из `u64` без использования `format!` и без heap-аллокации.
    pub fn from_u64(n: u64) -> Self {
        let mut buf = [0u8; 20];
        let slice = write_u64(n, &mut buf);

        Self::from_bytes(slice)
    }

    /// Создаёт `Sds` из `f64` без heap-аллокации.
    pub fn from_f64(n: f64) -> Self {
        use std::fmt::Write as FmtWrite;

        let mut buf = StackFmtBuf::<32>::new();

        write!(buf, "{n}").expect("StackFmtBuf overflow");

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

        s.parse::<f64>().map_err(|_| SdsNumericError::InvalidFloat)
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
) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
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

    &buf[pos..20]
}

/// Записывает десятичное представление `n` (u64) в `buf[..20]`.
/// Возвращает количество записанных байт.
#[inline]
fn write_u64(
    mut n: u64,
    buf: &mut [u8; 20],
) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }

    let mut pos = 20usize;

    while n > 0 {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    &buf[pos..20]
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
            Self::InvalidFloat => write!(f, "ERR value is not an float or out of range"),
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

    #[test]
    fn from_u64_roundtrip() {
        for n in [0u64, 1, 42, 255, 65535, u64::MAX] {
            let s = Sds::from_u64(n);
            let back = s
                .to_u64()
                .unwrap_or_else(|e| panic!("roundtrip failed for {n}: {e}"));
            assert_eq!(n, back);
        }
    }

    #[test]
    fn from_f64_zero() {
        assert_eq!(Sds::from_f64(0.0).as_str().unwrap(), "0");
        assert_eq!(Sds::from_f64(-0.0).as_str().unwrap(), "-0");
    }

    #[test]
    fn from_f64_whole_numbers() {
        assert_eq!(Sds::from_f64(1.0).as_str().unwrap(), "1");
        assert_eq!(Sds::from_f64(-1.0).as_str().unwrap(), "-1");
        assert_eq!(Sds::from_f64(100.0).as_str().unwrap(), "100");
    }

    #[allow(clippy::approx_constant)]
    #[test]
    fn from_f64_fractions() {
        assert_eq!(Sds::from_f64(3.14).as_str().unwrap(), "3.14");
        assert_eq!(Sds::from_f64(-0.5).as_str().unwrap(), "-0.5");
    }

    #[test]
    fn from_f64_special_values() {
        assert_eq!(Sds::from_f64(f64::INFINITY).as_str().unwrap(), "inf");
        assert_eq!(Sds::from_f64(f64::NEG_INFINITY).as_str().unwrap(), "-inf");
        assert_eq!(Sds::from_f64(f64::NAN).as_str().unwrap(), "NaN");
    }

    #[test]
    fn to_i64_basic() {
        assert_eq!(Sds::from_str("0").to_i64(), Ok(0));
        assert_eq!(Sds::from_str("1").to_i64(), Ok(1));
        assert_eq!(Sds::from_str("-1").to_i64(), Ok(-1));
        assert_eq!(Sds::from_str("42").to_i64(), Ok(42));
        assert_eq!(Sds::from_str("-42").to_i64(), Ok(-42));
    }

    #[test]
    fn to_i64_max_min_boundaries() {
        assert_eq!(Sds::from_str("9223372036854775807").to_i64(), Ok(i64::MAX));
        assert_eq!(Sds::from_str("-9223372036854775808").to_i64(), Ok(i64::MIN));
    }

    #[test]
    fn to_i64_overflow_pos() {
        // i64::MAX + 1
        assert_eq!(
            Sds::from_str("9223372036854775808").to_i64(),
            Err(SdsNumericError::Overflow)
        );
    }

    #[test]
    fn to_i64_overflow_neg() {
        // i64::MIN - 1
        assert_eq!(
            Sds::from_str("-9223372036854775809").to_i64(),
            Err(SdsNumericError::Overflow)
        );
    }

    #[test]
    fn to_i64_overflow_large() {
        assert_eq!(
            Sds::from_str("99999999999999999999").to_i64(),
            Err(SdsNumericError::Overflow)
        );
    }

    #[test]
    fn to_i64_invalid_formats() {
        let invalid = [
            "abc", "1.5", "1e10", " 1", "1 ", "-", "+", "1-2", "1_000", "0x1F",
        ];
        for input in &invalid {
            let result = Sds::from_str(input).to_i64();
            assert!(
                matches!(result, Err(SdsNumericError::InvalidInteger)),
                "expected InvalidInteger for {input:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn to_i64_empty() {
        assert_eq!(Sds::default().to_i64(), Err(SdsNumericError::Empty));
    }

    #[test]
    fn to_i64_invalid_utf8() {
        let s = Sds::from_vec(vec![0xFF, 0xFE]);
        assert_eq!(s.to_i64(), Err(SdsNumericError::InvalidUtf8));
    }

    #[test]
    fn to_u64_basic() {
        assert_eq!(Sds::from_str("0").to_u64(), Ok(0));
        assert_eq!(Sds::from_str("42").to_u64(), Ok(42));
        assert_eq!(Sds::from_str("255").to_u64(), Ok(255));
    }

    #[test]
    fn to_u64_max() {
        assert_eq!(Sds::from_str("18446744073709551615").to_u64(), Ok(u64::MAX));
    }

    #[test]
    fn to_u64_overflow() {
        assert_eq!(
            Sds::from_str("18446744073709551616").to_u64(),
            Err(SdsNumericError::Overflow)
        );
    }

    #[test]
    fn to_u64_negative_rejected() {
        assert_eq!(
            Sds::from_str("-1").to_u64(),
            Err(SdsNumericError::InvalidInteger)
        );
    }

    #[test]
    fn to_u64_empty() {
        assert_eq!(Sds::default().to_u64(), Err(SdsNumericError::Empty));
    }

    #[test]
    fn to_f64_integers() {
        assert_eq!(Sds::from_str("0").to_f64(), Ok(0.0));
        assert_eq!(Sds::from_str("1").to_f64(), Ok(1.0));
        assert_eq!(Sds::from_str("-1").to_f64(), Ok(-1.0));
    }

    #[allow(clippy::approx_constant)]
    #[test]
    fn to_f64_fractions() {
        let v = Sds::from_str("3.14").to_f64().unwrap();
        assert!((v - 3.14f64).abs() < 1e-10);
    }

    #[test]
    fn to_f64_scientific() {
        assert_eq!(Sds::from_str("1e10").to_f64(), Ok(1e10f64));
        assert_eq!(Sds::from_str("-1e-5").to_f64(), Ok(-1e-5f64));
    }

    #[test]
    fn to_f64_special_values() {
        assert!(Sds::from_str("inf").to_f64().unwrap().is_infinite());
        assert!(Sds::from_str("-inf").to_f64().unwrap().is_sign_negative());
        assert!(Sds::from_str("NaN").to_f64().unwrap().is_nan());
    }

    #[test]
    fn to_f64_invalid() {
        assert_eq!(
            Sds::from_str("abc").to_f64(),
            Err(SdsNumericError::InvalidFloat)
        );
    }

    #[test]
    fn to_f64_empty() {
        assert_eq!(Sds::default().to_f64(), Err(SdsNumericError::Empty));
    }

    #[test]
    fn is_integer_valid() {
        for s in &[
            "0",
            "1",
            "-1",
            "+1",
            "42",
            "-42",
            "007",
            "9223372036854775807",
        ] {
            assert!(Sds::from_str(s).is_integer(), "expected true for {s:?}");
        }
    }

    #[test]
    fn is_integer_invalid() {
        let invalid = ["", "-", "+", "1.5", "1e10", " 1", "1 ", "abc", "1-2"];
        for s in &invalid {
            assert!(!Sds::from_str(s).is_integer(), "expected false for {s:?}");
        }
    }

    #[test]
    fn is_integer_on_heap_string() {
        // Проверяем что is_integer работает и на heap-строках.
        let long_num = "9".repeat(Sds::INLINE_CAP + 1);
        let s = Sds::from_str(&long_num);
        assert!(!s.is_inline(), "there must be a heap for this test");
        assert!(s.is_integer());
    }

    #[test]
    fn is_integer_on_heap_invalid() {
        let long_str = "a".repeat(Sds::INLINE_CAP + 1);
        let s = Sds::from_str(&long_str);
        assert!(!s.is_inline());
        assert!(!s.is_integer());
    }
}
