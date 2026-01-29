use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Пороговое значение для автоматического переключения sparse->dense.
pub const DEFAULT_SPARSE_THRESHOLD: usize = 3000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HllSparse {
    /// Карта ненулевых регистров: индекс -> значение
    registers: BTreeMap<u16, u8>,
    /// Порог для переключения на dense encoding
    threshold: usize,
}

impl HllSparse {
    pub fn new() -> Self {
        Self::with_threshold(DEFAULT_SPARSE_THRESHOLD)
    }

    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            registers: BTreeMap::new(),
            threshold,
        }
    }

    pub fn set_register(
        &mut self,
        index: u16,
        value: u8,
    ) -> bool {
        if value == 0 {
            return false;
        }

        match self.registers.get(&index) {
            Some(&current) if current >= value => false,
            _ => {
                self.registers.insert(index, value);
                true
            }
        }
    }

    pub fn get_register(
        &self,
        index: u16,
    ) -> u8 {
        self.registers.get(&index).copied().unwrap_or(0)
    }

    pub fn should_convert_to_dense(&self) -> bool {
        self.registers.len() > self.threshold
    }

    pub fn is_empty(&self) -> bool {
        self.registers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.registers.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u16, u8)> + '_ {
        self.registers.iter().map(|(&k, &v)| (k, v))
    }

    pub fn memory_footprint(&self) -> usize {
        // BTreeMap overhead: ~32 байта на узел (зависит от реализации)
        // + размер самой структуры
        size_of::<Self>() + self.registers.len() * 32
    }

    pub fn merge(
        &mut self,
        other: &HllSparse,
    ) {
        for (&index, &value) in other.registers.iter() {
            self.set_register(index, value);
        }
    }

    pub fn clear(&mut self) {
        self.registers.clear();
    }

    pub fn count_zeros(
        &self,
        total_register: usize,
    ) -> usize {
        total_register - self.registers.len()
    }
}

impl Default for HllSparse {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_sparse() {
        let sparse = HllSparse::new();
        assert!(sparse.is_empty());
        assert_eq!(sparse.len(), 0);
        assert_eq!(sparse.threshold, DEFAULT_SPARSE_THRESHOLD);
    }

    #[test]
    fn test_set_and_get_register() {
        let mut sparse = HllSparse::new();

        // Установка значения
        assert!(sparse.set_register(100, 5));
        assert_eq!(sparse.get_register(100), 5);
        assert_eq!(sparse.len(), 1);

        // Попытка установить меньшее значение
        assert!(!sparse.set_register(100, 3));
        assert_eq!(sparse.get_register(100), 5);
        assert_eq!(sparse.len(), 1);

        // Установка большего значения
        assert!(sparse.set_register(100, 7));
        assert_eq!(sparse.get_register(100), 7);
        assert_eq!(sparse.len(), 1);

        // Получение неустановленного регистра
        assert_eq!(sparse.get_register(200), 0);
    }

    #[test]
    fn test_zero_value_not_stored() {
        let mut sparse = HllSparse::new();

        // Нулевые значения не должны храниться
        assert!(!sparse.set_register(100, 0));
        assert!(sparse.is_empty());
    }

    #[test]
    fn test_should_convert_to_dense() {
        let mut sparse = HllSparse::with_threshold(10);

        // Добавляем меньше threshold
        for i in 0..10 {
            sparse.set_register(i, 1);
        }
        assert!(!sparse.should_convert_to_dense());

        // Добавляем больше threshold
        sparse.set_register(10, 1);
        assert!(sparse.should_convert_to_dense());
    }

    #[test]
    fn test_merge() {
        let mut sparse1 = HllSparse::new();
        sparse1.set_register(100, 5);
        sparse1.set_register(200, 3);

        let mut sparse2 = HllSparse::new();
        sparse2.set_register(100, 3); // меньше, чем в sparse1
        sparse2.set_register(200, 7); // больше, чем в sparse1
        sparse2.set_register(300, 4); // новый регистр

        sparse1.merge(&sparse2);

        assert_eq!(sparse1.get_register(100), 5); // max(5, 3)
        assert_eq!(sparse1.get_register(200), 7); // max(3, 7)
        assert_eq!(sparse1.get_register(300), 4); // новый
        assert_eq!(sparse1.len(), 3);
    }

    #[test]
    fn test_count_zeros() {
        let mut sparse = HllSparse::new();
        sparse.set_register(0, 1);
        sparse.set_register(1, 2);
        sparse.set_register(2, 3);

        assert_eq!(sparse.count_zeros(10), 7); // 10 - 3 = 7
    }

    #[test]
    fn test_memory_footprint() {
        let mut sparse = HllSparse::new();
        let empty_size = sparse.memory_footprint();

        // Добавляем несколько регистров
        for i in 0..100 {
            sparse.set_register(i, 1);
        }

        let filled_size = sparse.memory_footprint();
        assert!(filled_size > empty_size);
    }

    #[test]
    fn test_clear() {
        let mut sparse = HllSparse::new();
        sparse.set_register(100, 5);
        sparse.set_register(200, 3);

        assert!(!sparse.is_empty());
        sparse.clear();
        assert!(sparse.is_empty());
        assert_eq!(sparse.len(), 0);
    }

    #[test]
    fn test_iter() {
        let mut sparse = HllSparse::new();
        sparse.set_register(100, 5);
        sparse.set_register(50, 3);
        sparse.set_register(200, 7);

        let collected: Vec<_> = sparse.iter().collect();

        // BTreeMap гарантирует упорядоченность
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], (50, 3));
        assert_eq!(collected[1], (100, 5));
        assert_eq!(collected[2], (200, 7));
    }

    #[test]
    fn test_serialization() {
        let mut sparse = HllSparse::new();
        sparse.set_register(100, 5);
        sparse.set_register(200, 3);

        // Сериализация
        let serialized = bincode::serialize(&sparse).unwrap();

        // Десериализация
        let deserialized: HllSparse = bincode::deserialize(&serialized).unwrap();

        assert_eq!(sparse, deserialized);
    }
}
