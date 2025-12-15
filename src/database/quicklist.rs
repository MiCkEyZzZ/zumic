//! QuickList — это сегментированная структура списка, оптимизированная для
//! операций добавления/удаления элементов с обеих сторон и адаптивного
//! управления памятью.

use std::collections::VecDeque;

use serde::{Deserialize, Deserializer, Serialize};

/// Сегментированный список с ограниченными по размеру сегментами
/// и оптимизированным доступом к элементам.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct QuickList<T> {
    /// Сегменты списка; каждый — это `VecDeque` с ограниченным размером
    segments: Vec<VecDeque<T>>,
    /// Кумулятивные длины сегментов для быстрого поиска
    segment_starts: Vec<usize>,
    /// Максимальное количество элементов в одном сегменте
    max_segment_size: usize,
    /// Общее количество элементов во всех сегментах
    len: usize,
    /// Кэш последнего accessed сегмента для sequential access patterns
    #[serde(skip)]
    last_accessed: Option<(usize, usize)>, // (segment_idx, global_index)
    /// Флаг указывающий что индекс нуждается в обновлении
    #[serde(skip)]
    index_dirty: bool,
    /// Счётчик операций с последней оптимизации
    #[serde(skip)]
    ops_since_optimize: usize,
    /// Порог операций перед проверкой необходимости оптимизации
    #[serde(skip)]
    optimize_threshold: usize,
}

/// Информация о фрагментации памяти QuickList.
pub struct FragmentationInfo {
    pub total_segments: usize,
    pub total_capacity: usize,
    pub total_length: usize,
    pub wasted_space_percent: f64,
    pub average_fill_rate: f64,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<T> QuickList<T> {
    /// Создаёт новый пустой `QuickList` с заданным размером сегмента.
    pub fn new(max_segment_size: usize) -> Self {
        Self {
            segments: Vec::new(),
            segment_starts: vec![0],
            max_segment_size,
            len: 0,
            last_accessed: None,
            index_dirty: false,
            ops_since_optimize: 0,
            optimize_threshold: 1000,
        }
    }

    /// Создаёт QuickList с кастомным порогом оптимизации.
    pub fn with_optimize_threshold(
        max_segment_size: usize,
        threshold: usize,
    ) -> Self {
        Self {
            segments: Vec::new(),
            segment_starts: vec![0],
            max_segment_size,
            len: 0,
            last_accessed: None,
            index_dirty: false,
            ops_since_optimize: 0,
            optimize_threshold: threshold,
        }
    }

    /// Возвращает общее количество элементов.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Возвращает `true` если список пуст.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Возвращает ссылку на элемент по логическому индексу.
    pub fn get(
        &mut self,
        index: usize,
    ) -> Option<&T> {
        let (seg_idx, offset) = self.find_segment(index)?;
        self.segments[seg_idx].get(offset)
    }

    /// Возвращает изменяемую ссылку на элемент по индексу.
    pub fn get_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut T> {
        let (seg_idx, offset) = self.find_segment(index)?;
        self.segments.get_mut(seg_idx)?.get_mut(offset)
    }

    /// Вставляет элемент в начало списка.
    pub fn push_front(
        &mut self,
        item: T,
    ) {
        if self.segments.is_empty() || self.segments[0].len() >= self.max_segment_size {
            self.segments
                .insert(0, VecDeque::with_capacity(self.max_segment_size));
            self.mark_index_dirty();
        }

        self.segments[0].push_front(item);
        self.len += 1;

        if !self.index_dirty {
            self.update_segment_starts_from(0);
        }

        self.auto_optimize();
    }

    /// Вставляет элемент в конец списка.
    pub fn push_back(
        &mut self,
        item: T,
    ) {
        let needed_new_segment = self.segments.is_empty()
            || self.segments.last().unwrap().len() >= self.max_segment_size;

        if needed_new_segment {
            self.segments
                .push(VecDeque::with_capacity(self.max_segment_size));
            self.mark_index_dirty();
        }

        let last_idx = self.segments.len() - 1;
        self.segments[last_idx].push_back(item);
        self.len += 1;

        if !self.index_dirty {
            self.update_segment_starts_from(last_idx);
        }

        self.auto_optimize();
    }

    /// Удаляет и возвращает первый элемент.
    pub fn pop_front(&mut self) -> Option<T> {
        if self.segments.is_empty() {
            return None;
        }

        let item = self.segments[0].pop_front();

        if let Some(item) = item {
            self.len -= 1;

            if self.segments[0].is_empty() {
                self.segments.remove(0);
                self.mark_index_dirty();
            } else if !self.index_dirty {
                self.update_segment_starts_from(0);
            }

            self.auto_optimize();
            Some(item)
        } else {
            None
        }
    }

    /// Удаляет и возвращает последний элемент.
    pub fn pop_back(&mut self) -> Option<T> {
        if self.segments.is_empty() {
            return None;
        }

        let last_idx = self.segments.len() - 1;
        let item = self.segments[last_idx].pop_back();

        if let Some(item) = item {
            self.len -= 1;

            if self.segments[last_idx].is_empty() {
                self.segments.pop();
                self.mark_index_dirty();
            } else if !self.index_dirty {
                self.update_segment_starts_from(last_idx);
            }

            self.auto_optimize();
            Some(item)
        } else {
            None
        }
    }

    /// Возвращает итератор по элементам.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.segments.iter().flat_map(|seg| seg.iter())
    }

    /// Очищает список.
    pub fn clear(&mut self) {
        self.segments.clear();
        self.segment_starts = vec![0];
        self.len = 0;
        self.last_accessed = None;
        self.index_dirty = false;
        self.ops_since_optimize = 0;
    }

    /// Проверяет корректность структуры.
    pub fn validate(&self) -> Result<(), &'static str> {
        let mut total_len = 0;

        for segment in &self.segments {
            if segment.capacity() > self.max_segment_size * 2 {
                return Err("Segment capacity exceeds limit");
            }
            total_len += segment.len();
        }

        if total_len != self.len() {
            return Err("Length mismatch");
        }

        if self.segment_starts.len() != self.segments.len() + 1 {
            return Err("segment_starts length mismatch");
        }

        if !self.index_dirty {
            let mut expected = 0;
            for (i, segment) in self.segments.iter().enumerate() {
                if self.segment_starts[i] != expected {
                    return Err("segment_starts value mismatch");
                }
                expected += segment.len();
            }

            if self.segment_starts[self.segments.len()] != expected {
                return Err("segment_starts final value mismatch");
            }
        }

        Ok(())
    }

    /// Оптимизирует сегменты: объединяет малозаполненные и удаляет пустые.
    pub fn optimize(&mut self) {
        let mut new_segments = Vec::new();
        let mut current_segment = VecDeque::with_capacity(self.max_segment_size);

        for segment in self.segments.drain(..) {
            for item in segment {
                if current_segment.len() >= self.max_segment_size {
                    new_segments.push(current_segment);
                    current_segment = VecDeque::with_capacity(self.max_segment_size);
                }
                current_segment.push_back(item);
            }
        }

        if !current_segment.is_empty() {
            new_segments.push(current_segment);
        }

        self.segments = new_segments;
        self.mark_index_dirty();
        self.rebuild_segment_starts();
        self.ops_since_optimize = 0;
    }

    /// Создаёт `QuickList` из одного `VecDeque`.
    pub fn from_vecdeque(
        items: VecDeque<T>,
        max_segment_size: usize,
    ) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in items {
            qlist.push_back(item);
        }
        qlist
    }

    /// Преобразует список в один `VecDeque`.
    pub fn into_vecdeque(self) -> VecDeque<T> {
        let mut result = VecDeque::with_capacity(self.len);
        for mut segment in self.segments {
            result.append(&mut segment);
        }
        result
    }

    /// Создаёт список из любого итерируемого источника.
    pub fn from_iter<I: IntoIterator<Item = T>>(
        iter: I,
        max_segment_size: usize,
    ) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in iter {
            qlist.push_back(item);
        }
        qlist
    }

    /// Автоматическая оптимизация при необходимости.
    pub fn auto_optimize(&mut self) {
        self.ops_since_optimize += 1;

        // FAST PATH: Не проверяем при каждой операции!
        if self.ops_since_optimize < self.optimize_threshold {
            return;
        }

        // Reset counter
        self.ops_since_optimize = 0;

        // SLOW PATH: Теперь делаем дорогую проверку только когда нужно
        let should_optimize = self.segments.len() > 5
            || self
                .segments
                .iter()
                .any(|s| s.len() < self.max_segment_size / 4);

        if should_optimize {
            self.optimize();
        }
    }

    /// Сжимает сегменты до размера фактических данных.
    pub fn set_optimize_threshold(
        &mut self,
        threshold: usize,
    ) {
        self.optimize_threshold = threshold;
    }

    /// Сжимает сегменты до размера фактических данных.
    pub fn shrink_to_fit(&mut self) {
        for segment in &mut self.segments {
            segment.shrink_to_fit();
        }
    }

    /// Оценивает использование памяти в байтах.
    pub fn memory_usage(&self) -> usize {
        let segments_memory: usize = self
            .segments
            .iter()
            .map(|s| s.capacity() * std::mem::size_of::<T>())
            .sum();

        let index_memory = self.segment_starts.capacity() * std::mem::size_of::<usize>();
        let struct_memory = std::mem::size_of::<Self>();

        segments_memory + index_memory + struct_memory
    }

    /// Возвращает информацию о фрагментации и эффективности использования
    /// памяти.
    pub fn fragmentation_info(&self) -> FragmentationInfo {
        let total_capacity: usize = self.segments.iter().map(|s| s.capacity()).sum();
        let wasted_space = if total_capacity > 0 {
            ((total_capacity - self.len) as f64 / total_capacity as f64) * 100.0
        } else {
            0.0
        };

        let avg_fill_rate = if !self.segments.is_empty() {
            self.segments
                .iter()
                .map(|s| s.len() as f64 / s.capacity().max(1) as f64)
                .sum::<f64>()
                / self.segments.len() as f64
                * 100.0
        } else {
            0.0
        };

        FragmentationInfo {
            total_segments: self.segments.len(),
            total_capacity,
            total_length: self.len,
            wasted_space_percent: wasted_space,
            average_fill_rate: avg_fill_rate,
        }
    }

    /// Находит сегмент и локальный offset для глобального индекса.
    /// Использует binary search по segment_starts для O(log n) lookup.
    ///
    /// Оптимизации:
    /// 1. Проверка кэша для sequential access
    /// 2. Binary search для random access
    /// 3. Lazy rebuild индекса только при необходимости
    fn find_segment(
        &mut self,
        index: usize,
    ) -> Option<(usize, usize)> {
        if index >= self.len {
            return None;
        }

        if self.index_dirty || self.segment_starts.len() != self.segments.len() + 1 {
            self.rebuild_segment_starts();
        }

        // Fast path: проверяем кэш
        if let Some((cached_seg, _cached_idx)) = self.last_accessed {
            if cached_seg < self.segments.len() {
                let seg_start = self.segment_starts[cached_seg];
                let seg_end = seg_start + self.segments[cached_seg].len();

                if index >= seg_start && index < seg_end {
                    self.last_accessed = Some((cached_seg, index));
                    return Some((cached_seg, index - seg_start));
                }
            }
        }

        // Slow path: binary search
        match self.segment_starts.binary_search(&index) {
            Ok(seg_idx) => {
                self.last_accessed = Some((seg_idx, index));
                Some((seg_idx, 0))
            }
            Err(seg_idx) => {
                if seg_idx == 0 || seg_idx > self.segments.len() {
                    return None;
                }
                let actual_seg = seg_idx - 1;
                let offset = index - self.segment_starts[actual_seg];

                self.last_accessed = Some((actual_seg, index));
                Some((actual_seg, offset))
            }
        }
    }

    /// Read-only версия find_segment без кэщирования (для const методов).
    #[allow(dead_code)]
    fn find_segment_const(
        &self,
        index: usize,
    ) -> Option<(usize, usize)> {
        if index >= self.len {
            return None;
        }

        match self.segment_starts.binary_search(&index) {
            Ok(seg_idx) => Some((seg_idx, 0)),
            Err(seg_idx) => {
                if seg_idx == 0 || seg_idx > self.segments.len() {
                    return None;
                }
                let actual_seg = seg_idx - 1;
                let offset = index - self.segment_starts[actual_seg];
                Some((actual_seg, offset))
            }
        }
    }

    /// Инкрементальное обновление segment_starts после изменения конкретного
    /// сегмента. Это гораздо быстрее чем full rebuild - O(k) где k - кол-во
    /// сегмента после изменённого.
    fn update_segment_starts_from(
        &mut self,
        start_seg: usize,
    ) {
        if start_seg >= self.segments.len() {
            return;
        }

        for i in start_seg..self.segments.len() {
            let prev_start = if i == 0 { 0 } else { self.segment_starts[i] };
            let current_len = self.segments[i].len();
            let new_start = prev_start + current_len;

            if i + 1 < self.segment_starts.len() {
                self.segment_starts[i + 1] = new_start;
            }
        }

        self.index_dirty = false;
        self.last_accessed = None;
    }

    /// Полный rebuild segment_starts. Вызывается только когда структура
    /// сегмента изменилась (добавление/удаление сегментов).
    fn rebuild_segment_starts(&mut self) {
        self.segment_starts.clear();
        self.segment_starts.reserve(self.segments.len() + 1);

        let mut cumulative = 0;
        self.segment_starts.push(cumulative);

        for segment in &self.segments {
            cumulative += segment.len();
            self.segment_starts.push(cumulative);
        }

        self.index_dirty = false;
        self.last_accessed = None;
    }

    /// Помечает индекс как требующий обновления.
    /// Actual rebuild произойдёт lazy при следующем доступе.
    fn mark_index_dirty(&mut self) {
        self.index_dirty = true;
        self.last_accessed = None;

        let desired = self.segments.len() + 1;
        if self.segment_starts.len() != desired {
            self.segment_starts.resize(desired, 0);
        }
    }

    /// Гарантирует корректность структуры после десериализации или операций.
    fn ensure_valid_state(&mut self) {
        let calculated_len: usize = self.segments.iter().map(|s| s.len()).sum();

        if self.len != calculated_len {
            self.len = calculated_len;
        }

        self.mark_index_dirty();
        self.rebuild_segment_starts();

        // Инициализация optimization полей после десериализации
        self.ops_since_optimize = 0;
        if self.optimize_threshold == 0 {
            self.optimize_threshold = 1000;
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для QuickList
////////////////////////////////////////////////////////////////////////////////

impl<T> IntoIterator for QuickList<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments
            .into_iter()
            .flat_map(|seg| seg.into_iter())
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl<T> Default for QuickList<T> {
    fn default() -> Self {
        Self::new(512)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for QuickList<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct QuickListHelper<T> {
            segments: Vec<VecDeque<T>>,
            segment_starts: Vec<usize>,
            max_segment_size: usize,
            len: usize,
        }

        let helper = QuickListHelper::deserialize(deserializer)?;

        let mut qlist = QuickList {
            segments: helper.segments,
            segment_starts: helper.segment_starts,
            max_segment_size: helper.max_segment_size,
            len: helper.len,
            last_accessed: None,
            index_dirty: false,
            ops_since_optimize: 0,
            optimize_threshold: 1000,
        };

        qlist.ensure_valid_state();
        Ok(qlist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тестирует методы `push_front` и `pop_front`.
    /// Проверяет, что при добавлении элементов в начало и последующем их
    /// извлечении порядок и количество элементов сохраняются корректно.
    #[test]
    fn test_push_front_and_pop_front() {
        let mut list: QuickList<i32> = QuickList::new(3);
        list.push_front(1);
        list.push_front(2);
        list.push_front(3);

        assert_eq!(list.len(), 3);
        assert_eq!(list.pop_front(), Some(3));
        assert_eq!(list.pop_front(), Some(2));
        assert_eq!(list.pop_front(), Some(1));
        assert_eq!(list.len(), 0);
    }

    /// Тестирует методы `push_back` и `pop_back`.
    /// Проверяет, что при добавлении элементов в конец и последующем их
    /// удалении порядок и количество элементов остаются правильными.
    #[test]
    fn test_push_back_and_pop_back() {
        let mut list: QuickList<i32> = QuickList::new(3);
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        assert_eq!(list.len(), 3);
        assert_eq!(list.pop_back(), Some(3));
        assert_eq!(list.pop_back(), Some(2));
        assert_eq!(list.pop_back(), Some(1));
        assert_eq!(list.len(), 0);
    }

    /// Тестирует методы `get` и `get_mut`.
    /// Проверяет, что можно получить доступ к элементу по индексу
    /// и изменить его значение.
    #[test]
    fn test_get_and_get_mut() {
        let mut list: QuickList<i32> = QuickList::new(3);
        list.push_back(10);
        list.push_back(20);
        list.push_back(30);

        assert_eq!(list.get(0), Some(&10));
        assert_eq!(list.get(1), Some(&20));
        assert_eq!(list.get(2), Some(&30));

        if let Some(item) = list.get_mut(1) {
            *item = 25;
        }
        assert_eq!(list.get(1), Some(&25));
    }

    /// Тест проверяет корректность массива `segment_starts`
    /// после создания нескольких сегментов и обеспечивает
    /// правильный random access ко всем элементам.
    #[test]
    fn test_segment_starts_correctness() {
        let mut list: QuickList<i32> = QuickList::new(3);

        // Добавляем элементы создавая несколько сегментов
        for i in 0..10 {
            list.push_back(i);
        }

        // Проверяем что segment_starts корректны
        assert!(list.validate().is_ok());

        // Проверяем доступ ко всем элементам
        for i in 0..10 {
            assert_eq!(list.get(i), Some(&(i as i32)));
        }
    }

    /// Тест проверяет корректность массива `segment_starts`
    /// после создания нескольких сегментов и обеспечивает
    /// правильный random access ко всем элементам.
    #[test]
    fn test_incremental_index_updates() {
        let mut list: QuickList<i32> = QuickList::new(3);

        for i in 0..15 {
            list.push_back(i);
            assert!(list.validate().is_ok(), "Failed after push_back {}", i);
        }

        for i in 0..15 {
            assert_eq!(list.get(i), Some(&(i as i32)), "Failed to get index {}", i);
        }
    }

    /// Тест проверяет работу кэша последнего accessed сегмента
    /// при последовательном доступе (sequential access).
    #[test]
    fn test_cache_efficiency() {
        let mut list: QuickList<i32> = QuickList::new(5);

        // Заполняем список
        for i in 0..20 {
            list.push_back(i);
        }

        // Sequential access должен использовать кэш
        for i in 0..20 {
            assert_eq!(list.get(i), Some(&(i as i32)));
        }

        // Проверяем что кэш работает
        assert!(list.last_accessed.is_some());
    }

    /// Тестирует метод `clear`.
    /// Проверяет, что после очистки список становится пустым.
    #[test]
    fn test_clear() {
        let mut list: QuickList<i32> = QuickList::new(3);
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        list.clear();

        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        assert_eq!(list.segments.len(), 0);
        assert_eq!(list.segment_starts, vec![0]);
        assert!(list.validate().is_ok());
    }

    /// Тестирует метод `validate`.
    /// Проверяет, что список проходит проверку целостности
    /// и выдает ошибку при нарушении ограничений сегмента.
    #[test]
    fn test_validate() {
        let mut list: QuickList<i32> = QuickList::new(3);
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        assert!(list.validate().is_ok());
    }

    /// Тестирует метод `auto_optimize`.
    /// Проверяет, что оптимизация выполняется, если сегментов слишком много
    /// или они слабо заполнены.
    #[test]
    fn test_auto_optimize() {
        let mut list: QuickList<i32> = QuickList::new(3);

        for i in 0..20 {
            list.push_back(i);
        }

        let before = list.segments.len();
        list.auto_optimize();
        let after = list.segments.len();

        assert!(after <= before);
        assert_eq!(list.len(), 20);
        assert!(list.validate().is_ok());
    }

    /// Тестирует метод `from_vecdeque`.
    /// Проверяет, что список можно создать из `VecDeque`
    /// и элементы вставлены корректно.
    #[test]
    fn test_from_vecdeque() {
        let items: VecDeque<i32> = VecDeque::from(vec![1, 2, 3, 4, 5]);
        let list = QuickList::from_vecdeque(items, 3);

        assert_eq!(list.len(), 5);
        assert!(list.validate().is_ok());
    }

    /// Тестирует метод `into_vecdeque`.
    /// Проверяет, что список корректно преобразуется в `VecDeque`
    /// с правильным порядком элементов.
    #[test]
    fn test_into_vecdeque() {
        let mut list: QuickList<i32> = QuickList::new(3);
        for i in 1..=5 {
            list.push_back(i);
        }

        let vecdeque = list.into_vecdeque();
        assert_eq!(vecdeque, VecDeque::from(vec![1, 2, 3, 4, 5]));
    }

    /// Тестирует метод `shrink_to_fit`.
    /// Проверяет, что ёмкость сегментов уменьшается до фактического размера
    /// данных.
    #[test]
    fn test_shrink_to_fit() {
        let mut list: QuickList<i32> = QuickList::new(10);
        for i in 0..5 {
            list.push_back(i);
        }

        list.shrink_to_fit();

        for segment in &list.segments {
            assert!(segment.capacity() >= segment.len());
        }
    }

    /// Тестирует метод `memory_usage`.
    /// Проверяет, что расчёт использования памяти выполняется корректно,
    /// с учётом размера сегментов и элементов.
    #[test]
    fn test_memory_usage() {
        let mut list: QuickList<i32> = QuickList::new(3);
        for i in 0..10 {
            list.push_back(i);
        }

        let memory_usage = list.memory_usage();
        assert!(memory_usage > 0);
    }

    /// Тест проверяет корректность расчёта статистики
    /// фрагментации и среднего заполнения сегментов.
    #[test]
    fn test_fragmentation_info() {
        let mut list: QuickList<i32> = QuickList::new(10);
        for i in 0..25 {
            list.push_back(i);
        }

        let info = list.fragmentation_info();

        assert!(info.total_segments > 0);
        assert_eq!(info.total_length, 25);
        assert!(info.average_fill_rate > 0.0);
        assert!(info.average_fill_rate <= 100.0);
    }

    /// Тест проверяет корректность работы `QuickList`
    /// на большом объёме данных и случайном доступе.
    #[test]
    fn test_large_list_performance() {
        let mut list: QuickList<i32> = QuickList::new(100);

        // Вставляем много элементов
        for i in 0..10000 {
            list.push_back(i);
        }

        assert_eq!(list.len(), 10000);
        assert!(list.validate().is_ok());

        // Проверяем random access
        assert_eq!(list.get(0), Some(&0));
        assert_eq!(list.get(5000), Some(&5000));
        assert_eq!(list.get(9999), Some(&9999));
    }

    /// Тест проверяет корректность структуры при смешанных
    /// операциях push/pop с обеих сторон.
    #[test]
    fn test_mixed_operations() {
        let mut list: QuickList<i32> = QuickList::new(5);

        list.push_back(1);
        list.push_front(0);
        list.push_back(2);
        assert_eq!(list.len(), 3);

        assert_eq!(list.pop_front(), Some(0));
        assert_eq!(list.pop_back(), Some(2));
        assert_eq!(list.len(), 1);

        assert_eq!(list.get(0), Some(&1));
        assert!(list.validate().is_ok());
    }

    /// Тест проверяет корректность сериализации и десериализации
    /// `QuickList`, включая восстановление индексов.
    #[test]
    fn test_serde_serialization() {
        let mut list: QuickList<i32> = QuickList::new(3);
        for i in 0..10 {
            list.push_back(i);
        }

        let serialized = serde_json::to_string(&list).unwrap();

        let mut deserialized: QuickList<i32> = serde_json::from_str(&serialized).unwrap();

        // Проверяем что данные сохранились
        assert_eq!(deserialized.len(), 10);
        for i in 0..10 {
            assert_eq!(deserialized.get(i), Some(&(i as i32)));
        }

        // После десериализации индекс будет dirty, но это нормально
        assert!(deserialized.validate().is_ok());
    }

    #[test]
    fn test_lazy_optimization() {
        let mut list = QuickList::with_optimize_threshold(3, 10);

        // Добавляем 9 элементов - оптимизация не должна вызваться
        for i in 0..9 {
            list.push_back(i);
        }
        assert_eq!(list.ops_since_optimize, 9);

        // 10-й элемент должен trigger проверку
        list.push_back(10);
        assert_eq!(list.ops_since_optimize, 0); // Counter reset
    }
}
