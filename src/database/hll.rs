use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Количество регистров в HyperLogLog
/// (обычно степень двойки, здесь 16 384).
const NUM_REGISTERS: usize = 16_384;
/// Ширина каждого регистра в битах (6 бит
/// на регистр для хранения значения rho).
const REGISTER_BITS: usize = 6;
/// Общий размер массива регистров в байтах:
/// NUM_REGISTERS × REGISTER_BITS / 8 = 12 288 байт.
pub const DENSE_SIZE: usize = NUM_REGISTERS * REGISTER_BITS / 8; // 12288 байт

/// HyperLogLog — структура для приближённого
/// подсчёта мощности множества.
///
/// Использует NUM_REGISTERS регистров, каждый
/// из которых хранит максимум rho бит.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hll {
    /// Упакованные регистры: массив байт длиной DENSE_SIZE.
    #[serde(with = "BigArray")]
    pub data: [u8; DENSE_SIZE],
}

impl Hll {
    /// Создаёт новый пустой HLL — все регистры обнулены.
    pub fn new() -> Self {
        Self {
            data: [0; DENSE_SIZE],
        }
    }

    /// Добавляет элемент `value` в структуру:
    /// 1. Хэширует значение в 64-битный хэш.
    /// 2. Делит хэш на `index` (старшие 14 бит) и `rho` (количество ведущих нулей в оставшихся битах + 1).
    /// 3. Обновляет регистр под номером `index`, если вычисленное `rho` больше текущего.
    pub fn add(
        &mut self,
        value: &[u8],
    ) {
        let hash = Self::hash(value);
        let (index, rho) = Self::index_and_rho(hash);
        let current = self.get_register(index);
        if rho > current {
            self.set_register(index, rho);
        }
    }

    /// Оценивает кардинальность множества (количество уникальных элементов).
    ///
    /// Алгоритм:
    /// 1. Вычисляет гармоническую сумму 1/2^register для всех регистров.
    /// 2. Применяет константу α_m = 0.7213 / (1 + 1.079/m).
    /// 3. При малых оценках (≤ 2.5·m) и наличии нулевых регистров — использует коррекцию Лог-Лог.
    pub fn estimate_cardinality(&self) -> f64 {
        let mut sum = 0.0;
        let mut zeros = 0;

        for i in 0..NUM_REGISTERS {
            let val = self.get_register(i);
            if val == 0 {
                zeros += 1;
            }
            sum += 1.0 / (1_u64 << val) as f64;
        }

        let m = NUM_REGISTERS as f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let raw_estimate = alpha * m * m / sum;

        if raw_estimate <= 2.5 * m && zeros != 0 {
            m * (m / zeros as f64).ln()
        } else {
            raw_estimate
        }
    }

    /// Хэширует срез байт в 64-битное значение, используя стандартный Hasher.
    fn hash(value: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// Делит 64-битный хэш:
    /// - `index` ← старшие 14 бит (для выбора регистра);
    /// - `rho` ← количество ведущих нулей в оставшихся битах + 1.
    fn index_and_rho(hash: u64) -> (usize, u8) {
        let index = (hash >> (64 - 14)) as usize;
        let remaining = hash << 14 | 1 << 13;
        let rho = remaining.leading_zeros() as u8 + 1;
        (index, rho)
    }

    /// Считывает 6-битный регистр под номером `index`.
    fn get_register(
        &self,
        index: usize,
    ) -> u8 {
        let bit_index = index * 6;
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;

        let byte1 = self.data[byte_index];
        let byte2 = if byte_index + 1 < DENSE_SIZE {
            self.data[byte_index + 1]
        } else {
            0
        };

        let combined = ((byte2 as u16) << 8) | byte1 as u16;
        ((combined >> bit_offset) & 0x3F) as u8
    }

    /// Записывает 6-битное значение `value` в регистр `index`.
    fn set_register(
        &mut self,
        index: usize,
        value: u8,
    ) {
        let bit_index = index * 6;
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;

        // Собираем два байта в 16-битное слово
        let mut combined = (self.data[byte_index] as u16)
            | ((self.data.get(byte_index + 1).cloned().unwrap_or(0) as u16) << 8);
        combined &= !(0x3F << bit_offset);
        combined |= (value as u16 & 0x3F) << bit_offset;
        self.data[byte_index] = (combined & 0xFF) as u8;
        if byte_index + 1 < DENSE_SIZE {
            self.data[byte_index + 1] = (combined >> 8) as u8;
        }
    }
}

impl Default for Hll {
    /// По умолчанию создаёт новый пустой HLL.
    fn default() -> Self {
        Self::new()
    }
}
