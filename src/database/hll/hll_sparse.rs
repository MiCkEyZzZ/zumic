use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Пороговое значение для автоматического переключения sparse->dense.
pub const DEFAULT_SPARSE_THRESHOLD: usize = 3000;

/// Разреженное представление HyperLogLog с настраиваемой точностью.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HllSparse<const P: usize> {
    /// Карта ненулевых регистров: индекс -> значение
    registers: BTreeMap<usize, u8>,
    /// Порог для переключения на dense encoding
    threshold: usize,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<const P: usize> HllSparse<P> {
    /// Создаёт новый sparse HLL с порогом по умолчанию.
    pub fn new() -> Self {
        Self::with_threshold(DEFAULT_SPARSE_THRESHOLD)
    }

    /// Создаёт новый sparse HLL с заданным порогом.
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            registers: BTreeMap::new(),
            threshold,
        }
    }

    /// Устанавливает значение регистра.
    #[inline]
    pub fn set_register(
        &mut self,
        index: usize,
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

    /// Возвращает значение регистра (0 если не установлен).
    #[inline]
    pub fn get_register(
        &self,
        index: usize,
    ) -> u8 {
        self.registers.get(&index).copied().unwrap_or(0)
    }

    /// Проверяет, нужно ли конвертировать в dense.
    pub fn should_convert_to_dense(&self) -> bool {
        self.registers.len() > self.threshold
    }

    /// Проверяет, пустой ли sparse HLL.
    pub fn is_empty(&self) -> bool {
        self.registers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.registers.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        self.registers.iter().map(|(&k, &v)| (k, v))
    }

    /// Возвращает приблизительную память кучи, используемую BTreeMap в байтах.
    /// (не включает size_of::<HllSparse>(), чтобы не дублировать при суммарном
    /// подсчёте)
    pub fn memory_footprint(&self) -> usize {
        // Эвристика: ~32 байта на узел BTreeMap (можно скорректировать)
        const BTREE_NODE_OVERHEAD: usize = 32;
        self.registers.len().saturating_mul(BTREE_NODE_OVERHEAD)
    }

    pub fn merge(
        &mut self,
        other: &HllSparse<P>,
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

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для HllSparse
////////////////////////////////////////////////////////////////////////////////

impl<const P: usize> Default for HllSparse<P> {
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
    fn test_new_sparse() {
        let sparse = HllSparse::<14>::new();
        assert!(sparse.is_empty());
        assert_eq!(sparse.len(), 0);
        assert_eq!(sparse.threshold, DEFAULT_SPARSE_THRESHOLD);
    }

    #[test]
    fn test_set_and_get_register() {
        let mut sparse = HllSparse::<14>::new();

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
        let mut sparse = HllSparse::<14>::new();

        // Нулевые значения не должны храниться
        assert!(!sparse.set_register(100, 0));
        assert!(sparse.is_empty());
    }

    #[test]
    fn test_should_convert_to_dense() {
        let mut sparse = HllSparse::<14>::with_threshold(10);

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
        let mut sparse1 = HllSparse::<14>::new();
        sparse1.set_register(100, 5);
        sparse1.set_register(200, 3);

        let mut sparse2 = HllSparse::<14>::new();
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
        let mut sparse = HllSparse::<14>::new();
        sparse.set_register(0, 1);
        sparse.set_register(1, 2);
        sparse.set_register(2, 3);

        assert_eq!(sparse.count_zeros(10), 7); // 10 - 3 = 7
    }

    #[test]
    fn test_memory_footprint() {
        let mut sparse = HllSparse::<14>::new();
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
        let mut sparse = HllSparse::<14>::new();
        sparse.set_register(100, 5);
        sparse.set_register(200, 3);

        assert!(!sparse.is_empty());
        sparse.clear();
        assert!(sparse.is_empty());
        assert_eq!(sparse.len(), 0);
    }

    #[test]
    fn test_iter() {
        let mut sparse = HllSparse::<14>::new();
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
    fn test_different_precisions() {
        let sparse4 = HllSparse::<4>::new();
        let sparse14 = HllSparse::<14>::new();
        let sparse18 = HllSparse::<18>::new();

        // Все должны работать одинаково
        assert!(sparse4.is_empty());
        assert!(sparse14.is_empty());
        assert!(sparse18.is_empty());
    }

    #[test]
    fn test_serialization() {
        let mut sparse = HllSparse::<14>::new();
        sparse.set_register(100, 5);
        sparse.set_register(200, 3);

        // Сериализация
        let serialized = bincode::serialize(&sparse).unwrap();

        // Десериализация
        let deserialized: HllSparse<14> = bincode::deserialize(&serialized).unwrap();

        assert_eq!(sparse, deserialized);
    }
}
