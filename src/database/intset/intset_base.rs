/// Внутренний тип кодирования значений в `IntSet`.
///
/// Определяет, какой тип используется для хранения чисел:
/// - `Int16` — 16-битные целые,
/// - `Int32` — 32-битные целые,
/// - `Int64` — 64-битные целые.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Int16,
    Int32,
    Int64,
}

/// Итератор по всем элементам `IntSet`.
pub enum IntSetIter<'a> {
    Int16(std::slice::Iter<'a, i16>),
    Int32(std::slice::Iter<'a, i32>),
    Int64(std::slice::Iter<'a, i64>),
}

/// Итератор по диапазону элементов `IntSet` `[start, end]` (inclusive).
pub enum IntSetRangeIter<'a> {
    Int16(std::slice::Iter<'a, i16>),
    Int32(std::slice::Iter<'a, i32>),
    Int64(std::slice::Iter<'a, i64>),
    Empty,
}

/// Компактное множество уникальных целых чисел с адаптивным хранением.
///
/// - Хранит элементы в отсортированном порядке.
/// - Автоматически расширяет внутренний тип при вставке больших чисел.
/// - Эффективно использует память для маленьких чисел.
/// - Поддерживает быстрый поиск, вставку, удаление и итерацию.
pub struct IntSet {
    enc: Encoding,
    data16: Vec<i16>,
    data32: Vec<i32>,
    data64: Vec<i64>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl IntSet {
    pub fn new() -> Self {
        Self {
            enc: Encoding::Int16,
            data16: Vec::new(),
            data32: Vec::new(),
            data64: Vec::new(),
        }
    }

    /// Возвращает кол-во элементов во множестве.
    #[inline]
    pub fn len(&self) -> usize {
        match self.enc {
            Encoding::Int16 => self.data16.len(),
            Encoding::Int32 => self.data32.len(),
            Encoding::Int64 => self.data64.len(),
        }
    }

    /// Проверяет, пустое ли множество.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Проверяет, содержит ли значение во множестве.
    #[inline]
    pub fn contains(
        &self,
        v: i64,
    ) -> bool {
        match self.enc {
            Encoding::Int16 => {
                if v < i16::MIN as i64 || v > i16::MAX as i64 {
                    return false;
                }
                let x = v as i16;
                self.data16.binary_search(&x).is_ok()
            }
            Encoding::Int32 => {
                if v < i32::MIN as i64 || v > i32::MAX as i64 {
                    return false;
                }
                let x = v as i32;
                self.data32.binary_search(&x).is_ok()
            }
            Encoding::Int64 => self.data64.binary_search(&v).is_ok(),
        }
    }

    /// Вставляет значение во множество.
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

    /// Удаляет указанное значение из множества.
    pub fn remove(
        &mut self,
        v: i64,
    ) -> bool {
        match self.enc {
            Encoding::Int16 => {
                if v < i16::MIN as i64 || v > i16::MAX as i64 {
                    return false;
                }
                let x = v as i16;
                if let Ok(pos) = self.data16.binary_search(&x) {
                    self.data16.remove(pos);
                    true
                } else {
                    false
                }
            }
            Encoding::Int32 => {
                if v < i32::MIN as i64 || v > i32::MAX as i64 {
                    return false;
                }
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

    /// Создаёт итератор по всем элементам множества в отсортированном порядке.
    #[inline]
    pub fn iter(&self) -> IntSetIter<'_> {
        match self.enc {
            Encoding::Int16 => IntSetIter::Int16(self.data16.iter()),
            Encoding::Int32 => IntSetIter::Int32(self.data32.iter()),
            Encoding::Int64 => IntSetIter::Int64(self.data64.iter()),
        }
    }

    /// Создаёт обратный итератор по всем элементам множества (от большего к
    /// меньшему).
    #[inline]
    pub fn rev_iter(&self) -> impl DoubleEndedIterator<Item = i64> + ExactSizeIterator + '_ {
        self.iter().rev()
    }

    /// Создаёт итератор по диапазону значений `[start, end]` включительно.
    pub fn iter_range(
        &self,
        start: i64,
        end: i64,
    ) -> IntSetRangeIter<'_> {
        if start > end {
            return IntSetRangeIter::empty();
        }

        match self.enc {
            Encoding::Int16 => {
                let start_idx = self.find_range_start_i16(start);
                let end_idx = self.find_range_end_i16(end);

                if start_idx >= end_idx {
                    IntSetRangeIter::empty()
                } else {
                    IntSetRangeIter::Int16(self.data16[start_idx..end_idx].iter())
                }
            }
            Encoding::Int32 => {
                let start_idx = self.find_range_start_i32(start);
                let end_idx = self.find_range_end_i32(end);

                if start_idx >= end_idx {
                    IntSetRangeIter::empty()
                } else {
                    IntSetRangeIter::Int32(self.data32[start_idx..end_idx].iter())
                }
            }
            Encoding::Int64 => {
                let start_idx = self.find_range_start_i64(start);
                let end_idx = self.find_range_end_i64(end);

                if start_idx >= end_idx {
                    IntSetRangeIter::empty()
                } else {
                    IntSetRangeIter::Int64(self.data64[start_idx..end_idx].iter())
                }
            }
        }
    }

    /// Находит позицию начала диапазона для значения `start` в массиве `i16`.
    #[inline]
    fn find_range_start_i16(
        &self,
        start: i64,
    ) -> usize {
        if start < i16::MIN as i64 {
            return 0;
        }

        if start > i16::MAX as i64 {
            return self.data16.len();
        }

        let start_val = start as i16;
        match self.data16.binary_search(&start_val) {
            Ok(idx) => idx,
            Err(idx) => idx,
        }
    }

    /// Находит позицию конца диапазона для значения `end` в массиве `i16`
    /// (exclusive).
    #[inline]
    fn find_range_end_i16(
        &self,
        end: i64,
    ) -> usize {
        if end < i16::MIN as i64 {
            return 0;
        }

        if end > i16::MAX as i64 {
            return self.data16.len();
        }

        let end_val = end as i16;
        match self.data16.binary_search(&end_val) {
            Ok(idx) => idx + 1, // inclusive, так что +1
            Err(idx) => idx,
        }
    }

    /// Находит позицию начала диапазона для значения `start` в массиве `i32`.
    #[inline]
    fn find_range_start_i32(
        &self,
        start: i64,
    ) -> usize {
        if start < i32::MIN as i64 {
            return 0;
        }

        if start > i32::MAX as i64 {
            return self.data32.len();
        }

        let start_val = start as i32;
        match self.data32.binary_search(&start_val) {
            Ok(idx) => idx,
            Err(idx) => idx,
        }
    }

    /// Находит позицию конца диапазона для значения `end` в массиве `i32`.
    #[inline]
    fn find_range_end_i32(
        &self,
        end: i64,
    ) -> usize {
        if end < i32::MIN as i64 {
            return 0;
        }

        if end > i32::MAX as i64 {
            return self.data32.len();
        }

        let end_val = end as i32;
        match self.data32.binary_search(&end_val) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        }
    }

    /// Находит позицию начала диапазона для значения `start` в массиве `i64`.
    #[inline]
    fn find_range_start_i64(
        &self,
        start: i64,
    ) -> usize {
        match self.data64.binary_search(&start) {
            Ok(idx) => idx,
            Err(idx) => idx,
        }
    }

    /// Находит позицию конца диапазона для значения `end` в массиве `i64`.
    #[inline]
    fn find_range_end_i64(
        &self,
        end: i64,
    ) -> usize {
        match self.data64.binary_search(&end) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        }
    }

    /// Расширяет внутреннее кодирование для поддержки большёго диапазона
    fn upgrade(
        &mut self,
        new_enc: Encoding,
    ) {
        match (self.enc, new_enc) {
            (Encoding::Int16, Encoding::Int32) => {
                self.data32 = self.data16.iter().map(|&x| x as i32).collect();
                self.data16.clear();
            }
            (Encoding::Int16, Encoding::Int64) => {
                self.data64 = self.data16.iter().map(|&x| x as i64).collect();
                self.data16.clear();
            }
            (Encoding::Int32, Encoding::Int64) => {
                self.data64 = self.data32.iter().map(|&x| x as i64).collect();
                self.data32.clear();
            }
            _ => {}
        }
        self.enc = new_enc;
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для IntSet, IntSetIter
////////////////////////////////////////////////////////////////////////////////

impl<'a> Iterator for IntSetIter<'a> {
    type Item = i64;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntSetIter::Int16(iter) => iter.next().map(|&x| x as i64),
            IntSetIter::Int32(iter) => iter.next().map(|&x| x as i64),
            IntSetIter::Int64(iter) => iter.next().copied(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            IntSetIter::Int16(iter) => iter.size_hint(),
            IntSetIter::Int32(iter) => iter.size_hint(),
            IntSetIter::Int64(iter) => iter.size_hint(),
        }
    }
}

impl<'a> ExactSizeIterator for IntSetIter<'a> {
    #[inline]
    fn len(&self) -> usize {
        match self {
            IntSetIter::Int16(iter) => iter.len(),
            IntSetIter::Int32(iter) => iter.len(),
            IntSetIter::Int64(iter) => iter.len(),
        }
    }
}

impl<'a> DoubleEndedIterator for IntSetIter<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            IntSetIter::Int16(iter) => iter.next_back().map(|&x| x as i64),
            IntSetIter::Int32(iter) => iter.next_back().map(|&x| x as i64),
            IntSetIter::Int64(iter) => iter.next_back().copied(),
        }
    }
}

impl<'a> IntSetRangeIter<'a> {
    #[inline]
    fn empty() -> Self {
        IntSetRangeIter::Empty
    }
}

impl<'a> Iterator for IntSetRangeIter<'a> {
    type Item = i64;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntSetRangeIter::Int16(iter) => iter.next().map(|&x| x as i64),
            IntSetRangeIter::Int32(iter) => iter.next().map(|&x| x as i64),
            IntSetRangeIter::Int64(iter) => iter.next().copied(),
            IntSetRangeIter::Empty => None,
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            IntSetRangeIter::Int16(iter) => iter.size_hint(),
            IntSetRangeIter::Int32(iter) => iter.size_hint(),
            IntSetRangeIter::Int64(iter) => iter.size_hint(),
            IntSetRangeIter::Empty => (0, Some(0)),
        }
    }
}

impl<'a> ExactSizeIterator for IntSetRangeIter<'a> {
    #[inline]
    fn len(&self) -> usize {
        match self {
            IntSetRangeIter::Int16(iter) => iter.len(),
            IntSetRangeIter::Int32(iter) => iter.len(),
            IntSetRangeIter::Int64(iter) => iter.len(),
            IntSetRangeIter::Empty => 0,
        }
    }
}

impl<'a> DoubleEndedIterator for IntSetRangeIter<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            IntSetRangeIter::Int16(iter) => iter.next_back().map(|&x| x as i64),
            IntSetRangeIter::Int32(iter) => iter.next_back().map(|&x| x as i64),
            IntSetRangeIter::Int64(iter) => iter.next_back().copied(),
            IntSetRangeIter::Empty => None,
        }
    }
}

impl Default for IntSet {
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

    /// Тест проверяет zero-copy итератор
    #[test]
    fn test_iter_zero_copy() {
        let mut set = IntSet::new();
        set.insert(3);
        set.insert(1);
        set.insert(2);

        let items: Vec<_> = set.iter().collect();
        assert_eq!(items, vec![1, 2, 3]);
    }

    /// Тест проверяет ExactSizeIterator
    #[test]
    fn test_iter_exact_size() {
        let mut set = IntSet::new();
        for i in 0..10 {
            set.insert(i);
        }

        let mut iter = set.iter();
        assert_eq!(iter.len(), 10);
        iter.next();
        assert_eq!(iter.len(), 9);
    }

    /// Тест проверяет обратный итератор
    #[test]
    fn test_rev_iter() {
        let mut set = IntSet::new();
        set.insert(1);
        set.insert(2);
        set.insert(3);

        let items: Vec<_> = set.rev_iter().collect();
        assert_eq!(items, vec![3, 2, 1]);
    }

    /// Тест проверяет range iterator
    #[test]
    fn test_iter_range() {
        let mut set = IntSet::new();
        for i in 0..10 {
            set.insert(i);
        }

        let items: Vec<_> = set.iter_range(3, 6).collect();
        assert_eq!(items, vec![3, 4, 5, 6]);
    }

    /// Тест проверяет range iterator на границах
    #[test]
    fn test_iter_range_boundaries() {
        let mut set = IntSet::new();
        for i in 5..15 {
            set.insert(i);
        }

        // Start before range
        let items: Vec<_> = set.iter_range(0, 7).collect();
        assert_eq!(items, vec![5, 6, 7]);

        // End after range
        let items: Vec<_> = set.iter_range(12, 20).collect();
        assert_eq!(items, vec![12, 13, 14]);

        // Completely outside
        let items: Vec<_> = set.iter_range(0, 3).collect();
        assert_eq!(items, Vec::<i64>::new());

        let items: Vec<_> = set.iter_range(20, 30).collect();
        assert_eq!(items, Vec::<i64>::new());
    }

    /// Тест проверяет range iterator с обратным порядком границ
    #[test]
    fn test_iter_range_inverted() {
        let mut set = IntSet::new();
        for i in 0..10 {
            set.insert(i);
        }

        let items: Vec<_> = set.iter_range(6, 3).collect();
        assert_eq!(items, Vec::<i64>::new());
    }

    /// Тест проверяет range iterator на больших значениях (i32)
    #[test]
    fn test_iter_range_i32() {
        let mut set = IntSet::new();
        let base = i16::MAX as i64 + 1000;
        for i in 0..10 {
            set.insert(base + i);
        }

        let items: Vec<_> = set.iter_range(base + 3, base + 6).collect();
        assert_eq!(items, vec![base + 3, base + 4, base + 5, base + 6]);
    }

    /// Тест проверяет range iterator в обратном направлении
    #[test]
    fn test_iter_range_rev() {
        let mut set = IntSet::new();
        for i in 0..10 {
            set.insert(i);
        }

        let items: Vec<_> = set.iter_range(3, 6).rev().collect();
        assert_eq!(items, vec![6, 5, 4, 3]);
    }

    /// Тест проверяет итератор с пустым set
    #[test]
    fn test_iter_empty() {
        let set = IntSet::new();
        assert_eq!(set.iter().count(), 0);
        assert_eq!(set.rev_iter().count(), 0);
        assert_eq!(set.iter_range(0, 10).count(), 0);
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

    /// Тест проверяет производительность итератора (нет аллокаций)
    #[test]
    fn test_iter_no_allocations() {
        let mut set = IntSet::new();
        for i in 0..1000 {
            set.insert(i);
        }

        // Просто проверяем, что итератор работает без паники
        let count = set.iter().count();
        assert_eq!(count, 1000);

        // И обратный тоже
        let count = set.rev_iter().count();
        assert_eq!(count, 1000);
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
}
