//! Модуль bitmap - работает с битовыми массивами.
//!
//! Позволяет устанавливать, читать и выполнять побитовые операции.
//! Используется для команд SETBIT, GETBIT, BITCOUNT, BITOP и др.

use std::ops::{BitAnd, BitOr, BitXor, Not};

/// Lookup-таблица для подсчёта битов в байте (0..=255)
const BIT_COUNT_TABLE: [u8; 256] = [
    0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4, 1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5,
    1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6,
    1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6,
    2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7,
    1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6,
    2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7,
    2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7,
    3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7, 4, 5, 5, 6, 5, 6, 6, 7, 5, 6, 6, 7, 6, 7, 7, 8,
];

/// Структура для хранения битов, реализованная как вектор байт.
///
/// Позволяет работать с битами по индексам и выполнять побитовые
/// операции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    bytes: Vec<u8>,
}

impl Bitmap {
    /// Создаёт новый пустой Bitmap.
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Создаёт Bitmap с заданной длиной в битах (все биты обнулены).
    pub fn with_capacity(bits: usize) -> Self {
        let byte_len = bits.div_ceil(8);
        Self {
            bytes: vec![0u8; byte_len],
        }
    }

    /// Устанавливает бит по смещению `bit_offset` в значение `value`
    /// (true/false).
    ///
    /// Возвращает старое значение бита.
    pub fn set_bit(
        &mut self,
        bit_offset: usize,
        value: bool,
    ) -> bool {
        let byte_index = bit_offset / 8;
        let bit_index = bit_offset % 8;

        // Расширяем массив при необходимости
        if byte_index >= self.bytes.len() {
            self.bytes.resize(byte_index + 1, 0);
        }

        let byte = &mut self.bytes[byte_index];
        let mask = 1 << (7 - bit_index);
        let old = *byte & mask != 0;

        if value {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }

        old
    }

    /// Получает значение бита по смещению.
    pub fn get_bit(
        &self,
        bit_offset: usize,
    ) -> bool {
        let byte_index = bit_offset / 8;
        let bit_index = bit_offset % 8;

        if byte_index >= self.bytes.len() {
            return false;
        }

        let byte = self.bytes[byte_index];
        (byte >> (7 - bit_index)) & 1 == 1
    }

    /// Подсчитывает количество установленных битов в диапазоне `[start, end)` (в битах).
    pub fn bitcount(
        &self,
        start: usize,
        end: usize,
    ) -> usize {
        let end = end.min(self.bit_len());
        let start = start.min(end);
        let start_byte = start / 8;
        // end_byte — индекс первого «за» последним байтом диапазона
        let end_byte = end.div_ceil(8);

        // подсчёт дополнительных ссылок.
        let mut count = self.bytes[start_byte..end_byte]
            .iter()
            .map(|&b| BIT_COUNT_TABLE[b as usize] as usize)
            .sum();

        // коррекция для неполных байтов на границах.
        let start_bit = start % 8;
        let end_bit = end % 8;

        // если всё в одном байте, сразу точная маса
        if start_byte == end_byte - 1 {
            let mask = (0xFFu8 >> start_bit) & (0xFFu8 << (8 - end_bit));
            return BIT_COUNT_TABLE[(self.bytes[start_byte] & mask) as usize] as usize;
        }

        // Первый байт (начиная с битa start_bit)
        if start_bit != 0 {
            let mask = 0xFFu8 >> start_bit;
            count -= BIT_COUNT_TABLE[(self.bytes[start_byte] & !mask) as usize] as usize;
        }

        // Последний байт (заканчивая битом end_bit)
        if end_bit != 0 {
            let mask = 0xFFu8 << (8 - end_bit);
            count -= BIT_COUNT_TABLE[(self.bytes[end_byte - 1] & !mask) as usize] as usize;
        }
        count
    }

    /// Возвращает длину битмапа в битах.
    pub fn bit_len(&self) -> usize {
        self.bytes.len() * 8
    }

    /// Получает внутреннее представление как slice байт.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

// Побитовые бинарные операции

impl BitAnd for &Bitmap {
    type Output = Bitmap;

    fn bitand(
        self,
        rhs: Self,
    ) -> Self::Output {
        Bitmap {
            bytes: self
                .bytes
                .iter()
                .zip(rhs.bytes.iter())
                .map(|(a, b)| a & b)
                .collect(),
        }
    }
}

impl BitOr for &Bitmap {
    type Output = Bitmap;

    fn bitor(
        self,
        rhs: Self,
    ) -> Self::Output {
        let len = self.bytes.len().max(rhs.bytes.len());
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            let a = self.bytes.get(i).copied().unwrap_or(0);
            let b = rhs.bytes.get(i).copied().unwrap_or(0);
            result.push(a | b);
        }
        Bitmap { bytes: result }
    }
}

impl BitXor for &Bitmap {
    type Output = Bitmap;

    fn bitxor(
        self,
        rhs: Self,
    ) -> Self::Output {
        let len = self.bytes.len().max(rhs.bytes.len());
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            let a = self.bytes.get(i).copied().unwrap_or(0);
            let b = rhs.bytes.get(i).copied().unwrap_or(0);
            result.push(a ^ b);
        }
        Bitmap { bytes: result }
    }
}

impl Not for &Bitmap {
    type Output = Bitmap;

    fn not(self) -> Self::Output {
        Bitmap {
            bytes: self.bytes.iter().map(|b| !b).collect(),
        }
    }
}

impl Default for Bitmap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_get_bit() {
        let mut bitmap = Bitmap::new();
        assert!(!bitmap.set_bit(5, true));
        assert!(bitmap.get_bit(5));
        assert!(bitmap.set_bit(5, false));
        assert!(!bitmap.get_bit(5));
    }

    #[test]
    fn test_bitcount() {
        let mut bitmap = Bitmap::new();
        bitmap.set_bit(0, true);
        bitmap.set_bit(3, true);
        bitmap.set_bit(15, true);
        assert_eq!(bitmap.bitcount(0, 16), 3);
        assert_eq!(bitmap.bitcount(4, 15), 0);
    }

    #[test]
    fn test_bitop_and_or_xor() {
        let mut a = Bitmap::new();
        let mut b = Bitmap::new();

        a.set_bit(1, true);
        a.set_bit(3, true);
        b.set_bit(3, true);
        b.set_bit(4, true);

        let and = &a & &b;
        let or = &a | &b;
        let xor = &a ^ &b;

        assert!(and.get_bit(3));
        assert!(!and.get_bit(1));
        assert!(or.get_bit(1));
        assert!(or.get_bit(4));
        assert!(xor.get_bit(1));
        assert!(!xor.get_bit(3));
        assert!(xor.get_bit(4));
    }

    #[test]
    fn test_bitop_not() {
        let mut bitmap = Bitmap::with_capacity(8);
        bitmap.set_bit(1, true);
        bitmap.set_bit(7, true);
        let not = &bitmap.not();

        assert!(!not.get_bit(1));
        assert!(not.get_bit(0));
        assert!(!not.get_bit(7));
    }
}
