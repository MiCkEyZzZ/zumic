//! `IntSet` — компактное множество целых чисел с автоматическим выбором
//! размера хранения.
//!
//! Хранит уникальные целые числа в отсортированном виде,
//! используя минимально необходимое количество байт: `i16`, `i32` или 
//! `i64`,
//! в зависимости от наибольшего вставленного значения. При необходимости 
//! кодировка
//! автоматически расширяется (upcast) в большую: с `i16` на `i32`, и с `i32`
//! на `i64`.
//!
//! Структура поддерживает операции вставки, удаления, проверки наличия элемента,
//! а также итерацию по элементам в отсортированном порядке. Множество эффективно
//! использует память, автоматически адаптируя тип данных под размер хранимых 
//! значений.

/// Перечисление, определяющее доступные кодировки для хранения
/// значений в множестве.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Int16,
    Int32,
    Int64,
}

/// Структура множества, хранящего уникальные значения в отсортированном виде.
/// Множество поддерживает три возможных кодировки данных: `Int16`, `Int32`, и `Int64`.
/// В зависимости от значения, структура может автоматически изменять свою кодировку.
pub struct IntSet {
    enc: Encoding,
    data16: Vec<i16>, // используется, только если enc == Int16
    data32: Vec<i32>,
    data64: Vec<i64>,
}

impl IntSet {
    /// Создает новое пустое множество с кодировкой `Int16`.
    pub fn new() -> Self {
        Self {
            enc: Encoding::Int16,
            data16: Vec::new(),
            data32: Vec::new(),
            data64: Vec::new(),
        }
    }

    /// Возвращает количество элементов в множестве.
    pub fn len(&self) -> usize {
        match self.enc {
            Encoding::Int16 => self.data16.len(),
            Encoding::Int32 => self.data32.len(),
            Encoding::Int64 => self.data64.len(),
        }
    }

    /// Проверяет, содержится ли значение в множестве.
    pub fn contains(&self, v: i64) -> bool {
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

    /// Вставляет значение в множество. Если значение уже существует, не
    /// происходит добавления.
    pub fn insert(&mut self, v: i64) -> bool {
        // 1) Определяем нужную кодировку
        let need = if v >= i16::MIN as i64 && v <= i16::MAX as i64 {
            Encoding::Int16
        } else if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
            Encoding::Int32
        } else {
            Encoding::Int64
        };

        // 2) Если нужно расширить, мигрируем
        if (need as u8) > (self.enc as u8) {
            self.upgrade(need);
        }

        // 3) Вставляем через binary_search + Vec::insert
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

    /// Обновляет кодировку множества, если это необходимо, чтобы поддерживать
    /// большее значение.
    fn upgrade(&mut self, new_enc: Encoding) {
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

    /// Удаляет значение из множества.
    pub fn remove(&mut self, v: i64) -> bool {
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

    /// Возвращает итератор по элементам множества.
    /// Элементы возвращаются в отсортированном порядке.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверяет вставку значения в диапазоне i16,
    /// работу contains и правильное определение длины
    #[test]
    fn test_insert_and_contains_i16() {
        let mut set = IntSet::new();
        assert!(set.insert(123));
        assert!(set.contains(123));
        assert_eq!(set.len(), 1);
    }

    /// Проверяет вставку значения, выходящего за i16,
    /// должно произойти расширение до i32
    #[test]
    fn test_insert_and_contains_i32() {
        let mut set = IntSet::new();
        let val = i16::MAX as i64 + 1;
        assert!(set.insert(val));
        assert!(set.contains(val));
        assert_eq!(set.len(), 1);
        assert_eq!(set.enc, Encoding::Int32);
    }

    /// Проверяет вставку значения, выходящего за i32,
    /// должно произойти расширение до i64
    #[test]
    fn test_insert_and_contains_i64() {
        let mut set = IntSet::new();
        let val = i32::MAX as i64 + 1;
        assert!(set.insert(val));
        assert!(set.contains(val));
        assert_eq!(set.len(), 1);
        assert_eq!(set.enc, Encoding::Int64);
    }

    /// Проверяет цепочку апгрейдов кодировки:
    /// Int16 -> Int32 -> Int64
    #[test]
    fn test_encoding_upgrade_chain() {
        let mut set = IntSet::new();
        assert!(set.insert(i16::MAX as i64)); // Int16
        assert_eq!(set.enc, Encoding::Int16);

        assert!(set.insert(i16::MAX as i64 + 1)); // -> Int32
        assert_eq!(set.enc, Encoding::Int32);

        assert!(set.insert(i32::MAX as i64 + 1)); // -> Int64
        assert_eq!(set.enc, Encoding::Int64);

        assert_eq!(set.len(), 3);
    }

    /// Проверяет удаление существующего и несуществующего значения
    #[test]
    fn test_remove() {
        let mut set = IntSet::new();
        set.insert(100);
        set.insert(200);
        assert!(set.remove(100));
        assert!(!set.contains(100));
        assert_eq!(set.len(), 1);

        assert!(!set.remove(999)); // не было
    }

    /// Проверяет, что дубликаты не вставляются второй раз
    #[test]
    fn test_insert_duplicates() {
        let mut set = IntSet::new();
        assert!(set.insert(50));
        assert!(!set.insert(50)); // уже есть
        assert_eq!(set.len(), 1);
    }

    /// Проверяет, что итератор возвращает элементы в отсортированном порядке
    #[test]
    fn test_iter_ordered() {
        let mut set = IntSet::new();
        set.insert(3);
        set.insert(1);
        set.insert(2);
        let items: Vec<_> = set.iter().collect();
        assert_eq!(items, vec![1, 2, 3]);
    }

    /// Проверяет итерацию по множеству с несколькими элементами (i64)
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

    /// Проверяет поведение пустого множества
    #[test]
    fn test_empty_set() {
        let set = IntSet::new();
        assert_eq!(set.len(), 0);
        assert!(!set.contains(0));
        assert_eq!(set.iter().count(), 0);
    }

    /// Проверяет вставку крайних значений для i16, i32, i64
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
