//! Модуль `bitmap` предоставляет структуру `Bitmap` для
//! эффективной работы с битовыми массивами.
//!
//! Поддерживаются операции установки и получения битов по
//! индексу, подсчёта установленных битов в диапазоне и
//! побитовые логические операции (`AND`, `OR`, `XOR`, `NOT`)
//! между битовыми массивами.
//!
//! Используется, например, для реализации команд: `SETBIT`,
//! `GETBIT`, `BITCOUNT`, `BITOP` и др.

use std::ops::{BitAnd, BitOr, BitXor, Not};

use serde::{Deserialize, Serialize};

/// Lookup-таблица для подсчёта количества установленных битов
/// в байтах от 0 до 255.
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

/// Структура `Bitmap` — представляет динамический битовый массив.
///
/// Используется для хранения и обработки битов с возможностью
/// побитовых операций и подсчёта.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bitmap {
    pub bytes: Vec<u8>,
}

impl Bitmap {
    /// Создаёт новый пустой `Bitmap` без заранее выделенной
    /// памяти.
    ///
    /// Массив автоматически расширяется при установке битов.
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Создаёт `Bitmap` с заданной длиной в битах.
    ///
    /// Все биты инициализируются значением `false` (0).
    ///
    /// # Аргументы
    ///
    /// * `bits` — количество битов, которые нужно зарезервировать.
    pub fn with_capacity(bits: usize) -> Self {
        let byte_len = bits.div_ceil(8);
        Self {
            bytes: vec![0u8; byte_len],
        }
    }

    /// Устанавливает бит по заданному смещению `bit_offset` в
    /// значение `value`.
    ///
    /// При необходимости битовый массив автоматически расширяется.
    ///
    /// # Возвращает
    ///
    /// `true`, если значение бита **до изменения** было установлено,
    /// `false` — если нет.
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

    /// Возвращает значение бита по заданному смещению `bit_offset`.
    ///
    /// Если бит выходит за границы текущего массива, возвращается
    /// `false`.
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

    /// Подсчитывает количество установленных (`true`) битов в диапазоне
    /// `[start, end)`.
    ///
    /// При выходе границ за пределы длины массива, диапазон автоматически
    /// ограничивается.
    ///
    /// # Аргументы
    ///
    /// * `start` — начало диапазона (включительно).
    /// * `end` — конец диапазона (исключительно).
    pub fn bitcount(
        &self,
        start: usize,
        end: usize,
    ) -> usize {
        let end = end.min(self.bit_len());
        let start = start.min(end);
        if start >= end {
            return 0;
        }

        let start_byte = start / 8;
        let end_byte = (end - 1) / 8;

        // Если всё в одном байте, применяем один маск
        if start_byte == end_byte {
            let sb = start % 8;
            let eb = end % 8;
            // для eb==0 считаем, что нужно взять все биты до конца байта
            let mask = if eb == 0 {
                0xFFu8 >> sb
            } else {
                (0xFFu8 >> sb) & (0xFFu8 << (8 - eb))
            };
            return BIT_COUNT_TABLE[(self.bytes[start_byte] & mask) as usize] as usize;
        }

        // Первый (частичный) байт
        let sb = start % 8;
        let first_mask = 0xFFu8 >> sb;
        let mut count = BIT_COUNT_TABLE[(self.bytes[start_byte] & first_mask) as usize] as usize;

        // Все целые байты между
        for &b in &self.bytes[start_byte + 1..end_byte] {
            count += BIT_COUNT_TABLE[b as usize] as usize;
        }

        // Последний (частичный) байт
        let eb = end % 8;
        let last_mask = if eb == 0 { 0xFFu8 } else { 0xFFu8 << (8 - eb) };
        count + BIT_COUNT_TABLE[(self.bytes[end_byte] & last_mask) as usize] as usize
    }

    /// Возвращает длину битового массива в битах (всегда кратно 8).
    pub fn bit_len(&self) -> usize {
        self.bytes.len() * 8
    }

    /// Возвращает ссылку на внутренний байтовый массив (`&[u8]`).
    ///
    /// Полезно для сериализации, отправки по сети или хэширования.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

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

    /// Тест проверяет корректность установки и получения значений
    /// отдельных битов.
    #[test]
    fn test_set_get_bit() {
        let mut bitmap = Bitmap::new();
        assert!(!bitmap.set_bit(5, true));
        assert!(bitmap.get_bit(5));
        assert!(bitmap.set_bit(5, false));
        assert!(!bitmap.get_bit(5));
    }

    /// Тест проверяет подсчёт установленных битов в заданном диапазоне.
    #[test]
    fn test_bitcount() {
        let mut bitmap = Bitmap::new();
        bitmap.set_bit(0, true);
        bitmap.set_bit(3, true);
        bitmap.set_bit(15, true);
        assert_eq!(bitmap.bitcount(0, 16), 3);
        assert_eq!(bitmap.bitcount(4, 15), 0);
    }

    /// Тест проверяет побитовые операции `AND`, `OR`, `XOR` между двумя
    /// Bitmap.
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

    /// Тест проверяет побитовую операцию `NOT` над Bitmap.
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
