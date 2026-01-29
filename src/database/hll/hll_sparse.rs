use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_SPARSE_THRESHOLD: usize = 3000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HllSparse {
    registers: BTreeMap<u16, u8>,
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

    pub fn is_empty(&self) -> bool {
        self.registers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.registers.len()
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

        // Попытка установить меньшее ззначение
        assert!(!sparse.set_register(100, 3));
        assert_eq!(sparse.get_register(100), 5);
        assert_eq!(sparse.len(), 1);

        // Установка большёго значения
        assert!(sparse.set_register(100, 7));
        assert_eq!(sparse.get_register(100), 7);
        assert_eq!(sparse.len(), 1);
    }

    #[test]
    fn test_zero_value_not_stored() {
        let mut sparse = HllSparse::new();

        // Нулевые значения не должны храниться
        assert!(!sparse.set_register(100, 0));
        assert!(sparse.is_empty());
    }
}
