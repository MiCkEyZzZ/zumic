use serde::{Deserialize, Serialize};

use crate::database::{dense_size, num_registers, HllSparse};

/// Плотное представление HyperLogLog с настраиваемой точностью.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HllDense<const P: usize> {
    pub data: Vec<u8>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<const P: usize> HllDense<P> {
    /// Создаёт новый пустой dense HLL.
    pub fn new() -> Self {
        let size = dense_size(P);
        Self {
            data: vec![0u8; size],
        }
    }

    /// Возвращает heap-часть, занятую dense-представлением:
    /// - размер структуры HllDense (Vec metadata) — будет на куче, т.к.
    ///   HllDense хранится в Box
    /// - плюс реальная capacity() в байтах (Vec<u8>)
    /// - плюс небольшой консервативный overhead для аллокатора
    pub fn memory_footprint(&self) -> usize {
        const ALLOC_OVERHEAD: usize = 32;
        // size_of::<Self>() — размер HllDense (в т.ч. метаданные Vec) — живёт в куче
        // под Box
        let struct_heap = std::mem::size_of::<Self>();
        let heap_data = self.data.capacity();
        struct_heap
            .saturating_add(heap_data)
            .saturating_add(ALLOC_OVERHEAD)
    }

    /// Записывает 6-битное значение `value` в регистр `index`.
    pub fn set_register(
        &mut self,
        index: usize,
        value: u8,
    ) {
        debug_assert!(index < num_registers(P), "Register index out of bounds");
        debug_assert!(value <= 63, "Register value must be <= 63 (6 bits)");

        let bit_index = index * 6;
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;

        // Собираем два байта в 16-битное слово
        let byte1 = self.data[byte_index];
        let byte2 = self.data.get(byte_index + 1).copied().unwrap_or(0);

        let mut combined = (byte1 as u16) | ((byte2 as u16) << 8);

        combined &= !(0x3F << bit_offset);
        combined |= (value as u16 & 0x3F) << bit_offset;

        // Записываем обратно
        self.data[byte_index] = (combined & 0xFF) as u8;
        if byte_index + 1 < self.data.len() {
            self.data[byte_index + 1] = (combined >> 8) as u8;
        }
    }

    /// Считывает 6-битный регистр под номером `index`.
    pub fn get_register(
        &self,
        index: usize,
    ) -> u8 {
        debug_assert!(index < num_registers(P), "Register index out of bounds");

        let bit_index = index * 6;
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;

        let byte1 = self.data[byte_index];
        let byte2 = self.data.get(byte_index + 1).copied().unwrap_or(0);

        let combined = (byte1 as u16) | ((byte2 as u16) << 8);
        ((combined >> bit_offset) & 0x3F) as u8
    }

    /// Создаёт dense представление из sparse.
    pub fn from_sparse(sparse: &HllSparse<P>) -> Self {
        let mut dense = Self::new();

        for (index, value) in sparse.iter() {
            dense.set_register(index, value);
        }
        dense
    }

    /// Возвращает размер данных в байтах
    #[inline]
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для HllDense
////////////////////////////////////////////////////////////////////////////////

impl<const P: usize> Default for HllDense<P> {
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_dense_is_zeroed() {
        let dense = HllDense::<14>::new();

        // Проверяем несколько регистров, включая крайние допустимые для P=4 (0..15)
        for idx in [0, 1, 10, 100, 500] {
            assert_eq!(dense.get_register(idx), 0);
        }
    }

    #[test]
    fn test_set_and_register_basic() {
        let mut dense = HllDense::<14>::new();

        dense.set_register(0, 5);
        dense.set_register(1, 17);
        dense.set_register(2, 63);

        assert_eq!(dense.get_register(0), 5);
        assert_eq!(dense.get_register(1), 17);
        assert_eq!(dense.get_register(2), 63);
    }

    #[test]
    fn test_register_overwrite() {
        let mut dense = HllDense::<14>::new();

        dense.set_register(10, 12);
        assert_eq!(dense.get_register(10), 12);

        dense.set_register(10, 31);
        assert_eq!(dense.get_register(10), 31)
    }

    #[test]
    fn test_cross_byte_registers() {
        // Индекс, у которого 6 бит пересекают границу байта
        // Например: index * 6 % 8 != 0
        let mut dense = HllDense::<14>::new();

        let index = 3; // 18 бит -> байт 2 + смещение 2
        dense.set_register(index, 45);

        assert_eq!(dense.get_register(index), 45);
    }

    #[test]
    fn test_adjacent_register_do_not_interface() {
        let mut dense = HllDense::<14>::new();

        dense.set_register(7, 11);
        dense.set_register(8, 22);
        dense.set_register(9, 33);

        assert_eq!(dense.get_register(7), 11);
        assert_eq!(dense.get_register(8), 22);
        assert_eq!(dense.get_register(9), 33);
    }

    #[test]
    fn test_max_register_value() {
        let mut dense = HllDense::<14>::new();

        dense.set_register(5, 63);
        assert_eq!(dense.get_register(5), 63);
    }

    #[test]
    fn test_from_sparse() {
        let mut sparse = HllSparse::<14>::new();

        sparse.set_register(1, 10);
        sparse.set_register(5, 20);
        sparse.set_register(12, 31);

        let dense = HllDense::<14>::from_sparse(&sparse);

        assert_eq!(dense.get_register(1), 10);
        assert_eq!(dense.get_register(5), 20);
        assert_eq!(dense.get_register(12), 31);

        // Неустановленные регистры должны быть нулями
        assert_eq!(dense.get_register(0), 0);
        assert_eq!(dense.get_register(2), 0);
    }

    #[test]
    fn test_dense_equality() {
        let mut d1 = HllDense::<14>::new();
        let mut d2 = HllDense::<14>::new();

        d1.set_register(3, 15);
        d2.set_register(3, 15);

        assert_eq!(d1, d2);

        d2.set_register(4, 1);
        assert_ne!(d1, d2);
    }

    #[test]
    fn test_dense_serde_roundtrip() {
        let mut dense = HllDense::<14>::new();
        dense.set_register(0, 7);
        dense.set_register(15, 42);
        dense.set_register(123, 31);

        let encoded = bincode::serialize(&dense).unwrap();
        let decoded: HllDense<14> = bincode::deserialize(&encoded).unwrap();

        assert_eq!(dense, decoded);
    }

    #[test]
    fn test_different_precisions() {
        let dense4 = HllDense::<4>::new();
        let dense14 = HllDense::<14>::new();
        let dense18 = HllDense::<18>::new();

        // Размеры должны соответствовать точности
        assert_eq!(dense4.size(), 12); // 16 * 6 / 8 = 12
        assert_eq!(dense14.size(), 12_288); // 16384 * 6 / 8 = 12288
        assert_eq!(dense18.size(), 196_608); // 262144 * 6 / 8 = 196608
    }

    #[test]
    fn test_precision_4_operations() {
        let mut dense = HllDense::<4>::new();

        // P=4 означает 16 регистров (2^4)
        for i in 0..16 {
            dense.set_register(i, (i as u8 + 1) % 64);
        }

        for i in 0..16 {
            assert_eq!(dense.get_register(i), (i as u8 + 1) % 64);
        }
    }

    #[test]
    fn test_precision_18_operations() {
        let mut dense = HllDense::<18>::new();

        // P=18 означает 262144 регистра
        // Тестируем несколько разрозненных регистров
        let test_indices = [0, 1000, 10000, 200000, 262143];

        for &idx in &test_indices {
            dense.set_register(idx, 42);
        }

        for &idx in &test_indices {
            assert_eq!(dense.get_register(idx), 42);
        }
    }
}
