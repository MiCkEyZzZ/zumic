//! Модуль bitmap - работает с битовыми массивами.
//!
//! Позволяет устанавливать, читать и выполнять побитовые операции.
//! Используется для команд SETBIT, GETBIT, BITCOUNT, BITOP и др.

use std::ops::{BitAnd, BitOr, BitXor, Not};

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
        (start..end).filter(|&i| self.get_bit(i)).count()
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
