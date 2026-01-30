use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::{database::HllSparse, DENSE_SIZE};

/// Плотное представление HyperLogLog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HllDense {
    #[serde(with = "BigArray")]
    data: [u8; DENSE_SIZE],
}

impl HllDense {
    /// Создаёт новый пустой dense HLL.
    pub fn new() -> Self {
        Self {
            data: [0; DENSE_SIZE],
        }
    }

    /// Записывает 6-битное значение `value` в регистр `index`.
    pub fn set_register(
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

    /// Считывает 6-битный регистр под номером `index`.
    pub fn get_register(
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

    /// Создаёт dense представление из sparse.
    pub fn from_sparse(sparse: &HllSparse) -> Self {
        let mut dense = Self::new();

        for (index, value) in sparse.iter() {
            dense.set_register(index as usize, value);
        }
        dense
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_dense_is_zeroed() {
        let dense = HllDense::new();

        // Проверяем несколько регистров, включая крайние
        for idx in [0, 1, 10, 100, 500] {
            assert_eq!(dense.get_register(idx), 0);
        }
    }

    #[test]
    fn test_set_and_register_basic() {
        let mut dense = HllDense::new();

        dense.set_register(0, 5);
        dense.set_register(1, 17);
        dense.set_register(2, 63);

        assert_eq!(dense.get_register(0), 5);
        assert_eq!(dense.get_register(1), 17);
        assert_eq!(dense.get_register(2), 63);
    }

    #[test]
    fn test_register_overwrite() {
        let mut dense = HllDense::new();

        dense.set_register(10, 12);
        assert_eq!(dense.get_register(10), 12);

        dense.set_register(10, 31);
        assert_eq!(dense.get_register(10), 31)
    }

    #[test]
    fn test_cross_byte_registers() {
        // Индекс, у которого 6 бит пересекают границу байта
        // Например: index * 6 % 8 != 0
        let mut dense = HllDense::new();

        let index = 3; // 18 бит -> байт 2 + смещение 2
        dense.set_register(index, 45);

        assert_eq!(dense.get_register(index), 45);
    }

    #[test]
    fn test_adjacent_register_do_not_interface() {
        let mut dense = HllDense::new();

        dense.set_register(7, 11);
        dense.set_register(8, 22);
        dense.set_register(9, 33);

        assert_eq!(dense.get_register(7), 11);
        assert_eq!(dense.get_register(8), 22);
        assert_eq!(dense.get_register(9), 33);
    }

    #[test]
    fn test_max_register_value() {
        let mut dense = HllDense::new();

        dense.set_register(5, 63);
        assert_eq!(dense.get_register(5), 63);
    }

    #[test]
    fn test_from_sparse() {
        let mut sparse = HllSparse::new();

        sparse.set_register(1, 10);
        sparse.set_register(5, 20);
        sparse.set_register(42, 31);

        let dense = HllDense::from_sparse(&sparse);

        assert_eq!(dense.get_register(1), 10);
        assert_eq!(dense.get_register(5), 20);
        assert_eq!(dense.get_register(42), 31);

        // Неустановленные регистры должны быть нулями
        assert_eq!(dense.get_register(0), 0);
        assert_eq!(dense.get_register(2), 0);
    }

    #[test]
    fn test_dense_equality() {
        let mut d1 = HllDense::new();
        let mut d2 = HllDense::new();

        d1.set_register(3, 15);
        d2.set_register(3, 15);

        assert_eq!(d1, d2);

        d2.set_register(4, 1);
        assert_ne!(d1, d2);
    }

    #[test]
    fn test_dense_serde_roundtrip() {
        let mut dense = HllDense::new();
        dense.set_register(0, 7);
        dense.set_register(15, 42);
        dense.set_register(123, 31);

        let encoded = bincode::serialize(&dense).unwrap();
        let decoded: HllDense = bincode::deserialize(&encoded).unwrap();

        assert_eq!(dense, decoded);
    }
}
