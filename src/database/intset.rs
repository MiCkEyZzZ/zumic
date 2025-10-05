//! Модуль компактного множества целых чисел с адаптивным хранением.
//!
//! `IntSet` — это структура для хранения уникальных целых чисел с минимальным
//! расходом памяти. Внутреннее представление автоматически выбирает наиболее
//! подходящий тип (`i16`, `i32`, `i64`) в зависимости от диапазона значений.
//! Все элементы всегда хранятся в отсортированном виде.

/// Внутренний тип хранения значений.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Int16,
    Int32,
    Int64,
}

/// Компактное множество уникальных целых чисел с адаптивным хранением.
///
/// Позволяет эффективно хранить небольшие и большие значения, автоматически
/// расширяя внутренний тип при необходимости. Все элементы уникальны и
/// отсортированы.
pub struct IntSet {
    enc: Encoding,
    data16: Vec<i16>,
    data32: Vec<i32>,
    data64: Vec<i64>,
}

impl IntSet {
    /// Создать пустое множество (начинает с i16).
    pub fn new() -> Self {
        Self {
            enc: Encoding::Int16,
            data16: Vec::new(),
            data32: Vec::new(),
            data64: Vec::new(),
        }
    }

    /// Количество элементов во множестве.
    pub fn len(&self) -> usize {
        match self.enc {
            Encoding::Int16 => self.data16.len(),
            Encoding::Int32 => self.data32.len(),
            Encoding::Int64 => self.data64.len(),
        }
    }

    /// Проверяет, пустое ли множество.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Проверить, есть ли значение во множестве.
    pub fn contains(
        &self,
        v: i64,
    ) -> bool {
        match self.enc {
            Encoding::Int16 => {
                let x = v as i16;
                self.data16.binary_search(&x).is_ok()
            }
            Encoding::Int32 => {
                let x = v as i32;
                self.data32.binary_search(&x).is_ok()
            }
            Encoding::Int64 => self.data64.binary_search(&v).is_ok(),
        }
    }

    /// Вставить значение. Возвращает true, если добавлено, false — если
    /// уже было.
    pub fn insert(
        &mut self,
        v: i64,
    ) -> bool {
        let need = if v >= i16::MIN as i64 && v <= i16::MAX as i64 {
            Encoding::Int16
        } else if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
            Encoding::Int32
        } else {
            Encoding::Int64
        };

        if (need as u8) > (self.enc as u8) {
            self.upgrade(need);
        }

        match self.enc {
            Encoding::Int16 => {
                let x = v as i16;
                match self.data16.binary_search(&x) {
                    Ok(_) => false,
                    Err(pos) => {
                        self.data16.insert(pos, x);
                        true
                    }
                }
            }
            Encoding::Int32 => {
                let x = v as i32;
                match self.data32.binary_search(&x) {
                    Ok(_) => false,
                    Err(pos) => {
                        self.data32.insert(pos, x);
                        true
                    }
                }
            }
            Encoding::Int64 => match self.data64.binary_search(&v) {
                Ok(_) => false,
                Err(pos) => {
                    self.data64.insert(pos, v);
                    true
                }
            },
        }
    }

    /// Upgrades the internal encoding to support larger values.
    fn upgrade(
        &mut self,
        new_enc: Encoding,
    ) {
        match (self.enc, new_enc) {
            (Encoding::Int16, Encoding::Int32) => {
                self.data32 = self.data16.iter().map(|&x| x as i32).collect();
            }
            (Encoding::Int16, Encoding::Int64) => {
                self.data64 = self.data16.iter().map(|&x| x as i64).collect();
            }
            (Encoding::Int32, Encoding::Int64) => {
                self.data64 = self.data32.iter().map(|&x| x as i64).collect();
            }
            _ => {}
        }
        self.enc = new_enc;
    }

    /// Удалить значение. Возвращает true, если было удалено.
    pub fn remove(
        &mut self,
        v: i64,
    ) -> bool {
        match self.enc {
            Encoding::Int16 => {
                let x = v as i16;
                if let Ok(pos) = self.data16.binary_search(&x) {
                    self.data16.remove(pos);
                    true
                } else {
                    false
                }
            }
            Encoding::Int32 => {
                let x = v as i32;
                if let Ok(pos) = self.data32.binary_search(&x) {
                    self.data32.remove(pos);
                    true
                } else {
                    false
                }
            }
            Encoding::Int64 => {
                if let Ok(pos) = self.data64.binary_search(&v) {
                    self.data64.remove(pos);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Итератор по элементам в отсортированном порядке (i64).
    pub fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        match self.enc {
            Encoding::Int16 => self
                .data16
                .iter()
                .map(|&x| x as i64)
                .collect::<Vec<_>>()
                .into_iter(),
            Encoding::Int32 => self
                .data32
                .iter()
                .map(|&x| x as i64)
                .collect::<Vec<_>>()
                .into_iter(),
            Encoding::Int64 => self.data64.clone().into_iter(),
        }
    }
}

impl Default for IntSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет вставку и проверку наличия значения в диапазоне i16
    #[test]
    fn test_insert_and_contains_i16() {
        let mut set = IntSet::new();
        assert!(set.insert(123));
        assert!(set.contains(123));
        assert_eq!(set.len(), 1);
    }

    /// Тест проверяет вставку значения вне диапазона i16, апгрейд до i32
    #[test]
    fn test_insert_and_contains_i32() {
        let mut set = IntSet::new();
        let val = i16::MAX as i64 + 1;
        assert!(set.insert(val));
        assert!(set.contains(val));
        assert_eq!(set.len(), 1);
        assert_eq!(set.enc, Encoding::Int32);
    }

    /// Тест проверяет вставку значения вне диапазона i32, апгрейд до i64
    #[test]
    fn test_insert_and_contains_i64() {
        let mut set = IntSet::new();
        let val = i32::MAX as i64 + 1;
        assert!(set.insert(val));
        assert!(set.contains(val));
        assert_eq!(set.len(), 1);
        assert_eq!(set.enc, Encoding::Int64);
    }

    /// Тест проверяет последовательный апгрейд: Int16 -> Int32 -> Int64
    #[test]
    fn test_encoding_upgrade_chain() {
        let mut set = IntSet::new();
        assert!(set.insert(i16::MAX as i64));
        assert_eq!(set.enc, Encoding::Int16);

        assert!(set.insert(i16::MAX as i64 + 1));
        assert_eq!(set.enc, Encoding::Int32);

        assert!(set.insert(i32::MAX as i64 + 1));
        assert_eq!(set.enc, Encoding::Int64);

        assert_eq!(set.len(), 3);
    }

    /// Тест проверяет удаление значения
    #[test]
    fn test_remove() {
        let mut set = IntSet::new();
        set.insert(100);
        set.insert(200);
        assert!(set.remove(100));
        assert!(!set.contains(100));
        assert_eq!(set.len(), 1);
        assert!(!set.remove(999));
    }

    /// Тест проверяет, что дубликаты не добавляются
    #[test]
    fn test_insert_duplicates() {
        let mut set = IntSet::new();
        assert!(set.insert(50));
        assert!(!set.insert(50));
        assert_eq!(set.len(), 1);
    }

    /// Тест проверяет итерацию по отсортированным элементам
    #[test]
    fn test_iter_ordered() {
        let mut set = IntSet::new();
        set.insert(3);
        set.insert(1);
        set.insert(2);
        let items: Vec<_> = set.iter().collect();
        assert_eq!(items, vec![1, 2, 3]);
    }

    /// Тест проверяет итерацию по большому диапазону
    #[test]
    fn test_iter_large() {
        let mut set = IntSet::new();
        for i in 1000..1010 {
            set.insert(i64::from(i));
        }
        let collected: Vec<_> = set.iter().collect();
        assert_eq!(
            collected,
            (1000..1010).map(|x| x as i64).collect::<Vec<_>>()
        );
    }

    /// Тест проверяет пустое множество
    #[test]
    fn test_empty_set() {
        let set = IntSet::new();
        assert_eq!(set.len(), 0);
        assert!(!set.contains(0));
        assert_eq!(set.iter().count(), 0);
    }

    /// Тест проверяет вставку граничных значений
    #[test]
    fn test_insert_max_min_edges() {
        let mut set = IntSet::new();
        let values = [
            i16::MIN as i64,
            i16::MAX as i64,
            i32::MIN as i64,
            i32::MAX as i64,
            i64::MIN,
            i64::MAX,
        ];
        for &v in &values {
            assert!(set.insert(v), "insert({v}) should succeed");
            assert!(set.contains(v), "contains({v}) should return true");
        }
        assert_eq!(set.len(), values.len());
    }
}
