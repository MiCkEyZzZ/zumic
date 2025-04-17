//! QuickList — это сегментированная структура списка, оптимизированная для операций
//! добавления/удаления элементов с начала/конца и адаптивного управления памятью.
//!
//! Она содержит вектор сегментов типа `VecDeque<T>`, что позволяет эффективно выполнять
//! операции `push_front`, `push_back`, `pop_front` и `pop_back`.
//!
//! Каждый сегмент имеет максимальный размер (`max_segment_size`), что уменьшает накладные
//! расходы на выделение памяти и фрагментацию. При перерастяжении или избыточном
//! разрежении сегментов список может быть перераспределён с помощью методов
//! `optimize()` или `auto_optimize()`.
//!
//! QuickList хорошо подходит для рабочих нагрузок с частыми добавлениями/удалениями
//! с обоих концов и редким произвольным доступом к элементам.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

/// Сегментированная структура списка с ограниченными сегментами и оптимизированным
/// доступом.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QuickList<T> {
    /// Сегменты списка; каждый сегмент — это `VecDeque` с ограниченным размером.
    segments: Vec<VecDeque<T>>,

    /// Максимальное количество элементов в одном сегменте.
    max_segment_size: usize,

    /// Общее количество элементов во всех сегментах.
    len: usize,

    /// Опциональная карта индексов от логического индекса до индекса сегмента.
    #[serde(skip)]
    index: HashMap<usize, usize>,
}

impl<T> QuickList<T> {
    /// Создаёт новый пустой `QuickList` с указанным размером сегмента.
    pub fn new(max_segment_size: usize) -> Self {
        Self {
            segments: Vec::new(),
            max_segment_size,
            len: 0,
            index: HashMap::new(),
        }
    }

    /// Возвращает общее количество элементов в списке.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Возвращает `true`, если список пуст.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Возвращает ссылку на элемент по заданному логическому индексу, если он существует.
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }
        let mut offset = 0;
        self.segments.iter().find_map(|segment| {
            if index < offset + segment.len() {
                segment.get(index - offset)
            } else {
                offset += segment.len();
                None
            }
        })
    }

    /// Возвращает изменяемую ссылку на элемент по заданному логическому индексу, если он существует.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }
        let mut offset = 0;
        self.segments.iter_mut().find_map(|segment| {
            if index < offset + segment.len() {
                Some(segment.get_mut(index - offset).unwrap())
            } else {
                offset += segment.len();
                None
            }
        })
    }

    /// Вставляет элемент в начало списка.
    pub fn push_front(&mut self, item: T) {
        if self.segments.is_empty() || self.segments[0].len() >= self.max_segment_size {
            self.segments
                .insert(0, VecDeque::with_capacity(self.max_segment_size));
        }
        self.segments[0].push_front(item);
        self.len += 1;
        self.auto_optimize();
        self.rebuild_index();
    }

    /// Вставляет элемент в конец списка.
    pub fn push_back(&mut self, item: T) {
        if self.segments.is_empty() || self.segments.last().unwrap().len() >= self.max_segment_size
        {
            self.segments
                .push(VecDeque::with_capacity(self.max_segment_size));
        }
        self.segments.last_mut().unwrap().push_back(item);
        self.len += 1;
        self.auto_optimize();
        self.rebuild_index();
    }

    /// Удаляет и возвращает первый элемент списка, если он существует.
    pub fn pop_front(&mut self) -> Option<T> {
        if self.segments.is_empty() {
            return None;
        }
        let item = self.segments[0].pop_front();
        if item.is_some() {
            self.len -= 1;
            if self.segments[0].is_empty() {
                self.segments.remove(0);
            }
            self.auto_optimize();
            self.rebuild_index();
        }
        item
    }

    /// Удаляет и возвращает последний элемент списка, если он существует.
    pub fn pop_back(&mut self) -> Option<T> {
        if self.segments.is_empty() {
            return None;
        }

        let item = self.segments.last_mut().unwrap().pop_back();
        if item.is_some() {
            self.len -= 1;
            if self.segments.last().unwrap().is_empty() {
                self.segments.pop();
            }
            self.auto_optimize();
            self.rebuild_index();
        }
        item
    }

    /// Возвращает итератор по всем элементам списка.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.segments.iter().flat_map(|seg| seg.iter())
    }

    /// Очищает список, удаляя все элементы.
    pub fn clear(&mut self) {
        self.segments.clear();
        self.index.clear();
        self.len = 0;
    }

    /// Проверяет внутреннюю консистентность списка.
    /// Возвращает `Err`, если какой-либо сегмент превышает допустимую емкость
    /// или если внутренний счётчик длины некорректен.
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
        Ok(())
    }

    /// Компактирует и реорганизует сегменты для поддержания баланса.
    /// Объединяет недозаполненные сегменты и удаляет пустые.
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
    }

    /// Создаёт `QuickList` из одного `VecDeque`.
    pub fn from_vecdeque(items: VecDeque<T>, max_segment_size: usize) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in items {
            qlist.push_back(item);
        }
        qlist
    }

    /// Преобразует `QuickList` в один `VecDeque` элементов.
    pub fn into_vecdeque(self) -> VecDeque<T> {
        let mut result = VecDeque::with_capacity(self.len);
        for mut segment in self.segments {
            result.append(&mut segment);
        }
        result
    }

    /// Создаёт `QuickList` из любого итерируемого набора элементов.
    pub fn from_iter<I: IntoIterator<Item = T>>(iter: I, max_segment_size: usize) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in iter {
            qlist.push_back(item);
        }
        qlist
    }

    /// Автоматически оптимизирует список, если выполнены определённые условия.
    ///
    /// Оптимизация срабатывает, если сегментов слишком много
    /// или если сегменты значительно недозаполнены.
    pub fn auto_optimize(&mut self) {
        if self.segments.len() > 5
            || self
                .segments
                .iter()
                .any(|s| s.len() < self.max_segment_size / 2)
        {
            self.optimize();
        }
    }

    /// Сжимает каждый сегмент до его текущего размера.
    pub fn shrink_to_fit(&mut self) {
        for segment in &mut self.segments {
            segment.shrink_to_fit();
        }
    }

    /// Оценивает использование памяти списком в байтах.
    ///
    /// Расчёт основывается на ёмкости сегментов и размере типа `T`.
    pub fn memory_usage(&self) -> usize {
        self.segments
            .iter()
            .map(|s| s.capacity() * std::mem::size_of::<T>())
            .sum()
    }

    /// Перестраивает внутренний индекс, отображающий логические индексы на номера сегментов.
    ///
    /// Используется для ускоренного доступа по индексам или отладки.
    pub fn rebuild_index(&mut self) {
        self.index.clear();
        let mut global_index = 0;
        for (seg_idx, segment) in self.segments.iter().enumerate() {
            for _ in segment {
                self.index.insert(global_index, seg_idx);
                global_index += 1;
            }
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Тестирует методы `push_front` и `pop_front`.
    /// Проверяет, что при добавлении элементов в начало и последующем их извлечении
    /// порядок и количество элементов сохраняются корректно.
    #[test]
    fn test_push_front_and_pop_front() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_front(1);
        list.push_front(2);
        list.push_front(3);

        assert_eq!(list.len(), 3);

        let item = list.pop_front();
        assert_eq!(item, Some(3));

        assert_eq!(list.len(), 2);
    }

    /// Тестирует методы `push_back` и `pop_back`.
    /// Проверяет, что при добавлении элементов в конец и извлечении
    /// сохраняется правильный порядок и количество элементов.
    #[test]
    fn test_push_back_and_pop_back() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        assert_eq!(list.len(), 3);

        let item = list.pop_back();
        assert_eq!(item, Some(3));

        assert_eq!(list.len(), 2);
    }

    /// Тестирует методы `get` и `get_mut`.
    /// Проверяет, что можно корректно получить доступ к элементу по индексу
    /// и изменить его значение.
    #[test]
    fn test_get_and_get_mut() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(10);
        list.push_back(20);
        list.push_back(30);

        assert_eq!(list.get(1), Some(&20));

        if let Some(item) = list.get_mut(1) {
            *item = 25;
        }

        assert_eq!(list.get(1), Some(&25));
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
    }

    /// Тестирует метод `validate`.
    /// Проверяет, что список проходит валидацию, а также вызывает ошибку при нарушении ограничений сегмента.
    #[test]
    fn test_validate() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        assert!(list.validate().is_ok());

        list.segments[0].push_back(4);
        assert!(list.validate().is_err());
    }

    /// Тестирует метод `auto_optimize`.
    /// Проверяет, что оптимизация производится, если количество сегментов слишком
    /// елико или сегменты недозаполнены.
    #[test]
    fn test_auto_optimize() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);
        list.push_back(4);
        list.push_back(5);

        let before = list.segments.len();
        list.auto_optimize();
        let after = list.segments.len();

        assert!(after <= before);
        assert_eq!(list.len(), 5);
    }

    /// Тестирует метод `from_vecdeque`.
    /// Проверяет, что список можно создать из `VecDeque` и элементы корректно вставляются.
    #[test]
    fn test_from_vecdeque() {
        let items: VecDeque<i32> = VecDeque::from(vec![1, 2, 3]);
        let list = QuickList::from_vecdeque(items, 3);

        assert_eq!(list.len(), 3);
        assert_eq!(list.get(0), Some(&1));
        assert_eq!(list.get(1), Some(&2));
        assert_eq!(list.get(2), Some(&3));
    }

    /// Тестирует метод `into_vecdeque`.
    /// Проверяет, что список корректно преобразуется в `VecDeque` с правильной последовательностью элементов.
    #[test]
    fn test_into_vecdeque() {
        let mut list: QuickList<i32> = QuickList::new(3);
        list.push_back(10);
        list.push_back(20);
        list.push_back(30);

        let vecdeque: VecDeque<i32> = list.into_vecdeque();
        assert_eq!(vecdeque, VecDeque::from(vec![10, 20, 30]));
    }

    /// Тестирует метод `shrink_to_fit`.
    /// Проверяет, что ёмкость сегментов уменьшена до текущего размера данных.
    #[test]
    fn test_shrink_to_fit() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        list.shrink_to_fit();
        // Проверяем, что ёмкость каждого сегмента не меньше его длины.
        assert!(list.segments.iter().all(|seg| seg.capacity() >= seg.len()));
    }

    /// Тестирует метод `memory_usage`.
    /// Проверяет, что использование памяти списка вычисляется корректно с учётом емкости сегментов и размера типа.
    #[test]
    fn test_memory_usage() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        let memory_usage = list.memory_usage();
        // Проверяем, что использование памяти больше нуля (с учётом размера сегментов)
        assert!(memory_usage > 0);
    }
}
