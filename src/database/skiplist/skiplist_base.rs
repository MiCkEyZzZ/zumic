use std::{fmt::Debug, marker::PhantomData, ptr::NonNull};

use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ValidationError;
use crate::validate;

/// Максимальный уровень пропускного списка.
const MAX_LEVEL: usize = 16;

/// Вероятностный коэффициент для определения уровня нового узла.
const P: f64 = 0.5;

/// Узел пропускного списка.
#[derive(Debug)]
pub struct Node<K, V> {
    key: K,
    value: V,
    forward: Vec<Option<NonNull<Node<K, V>>>>,
    backward: Option<NonNull<Node<K, V>>>,
}

/// SkipList — структура с головным узлом, текущим уровнем и количеством
/// элементов.
#[derive(Debug)]
pub struct SkipList<K, V> {
    /// Головной (dummy) узел; не содержит полезных данных.
    head: Box<Node<K, V>>,
    /// Текущий максимальный уровень.
    level: usize,
    /// Количество элементов (без головы).
    length: usize,
}

/// Итератор по узлам списка в прямом порядке.
pub struct SkipListIter<'a, K, V> {
    current: Option<NonNull<Node<K, V>>>,
    _marker: PhantomData<&'a Node<K, V>>,
}

/// Итератор по узлам списка в обратном порядке.
pub struct ReverseIter<'a, K, V> {
    current: Option<NonNull<Node<K, V>>>,
    head: *const Node<K, V>,
    _marker: PhantomData<&'a Node<K, V>>,
}

/// Итератор по диапазону в SkipList.
pub struct RangeIter<'a, K, V> {
    current: Option<NonNull<Node<K, V>>>,
    end: Option<K>, // Конечный (не включается) ключ диапазона
    _marker: PhantomData<&'a Node<K, V>>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<K, V> Node<K, V> {
    /// Создаёт новый узел с заданным уровнем.
    fn new(
        key: K,
        value: V,
        level: usize,
    ) -> Box<Self> {
        Box::new(Node {
            key,
            value,
            forward: vec![None; level],
            backward: None,
        })
    }

    /// Возвращает ссылку на ключ.
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Возвращает ссылку на значение.
    pub fn value(&self) -> &V {
        &self.value
    }
}

impl<K, V> SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    /// Создаёт новый пустой SkipList.
    pub fn new() -> Self {
        let head = Node::new(Default::default(), Default::default(), MAX_LEVEL);
        SkipList {
            head,
            level: 1,
            length: 0,
        }
    }
    /// Генерирует случайный уровень для нового узла.
    fn random_level() -> usize {
        let mut lvl = 1;
        let mut rng = rand::thread_rng();
        while rng.gen::<f64>() < P && lvl < MAX_LEVEL {
            lvl += 1;
        }
        lvl
    }

    /// Поиск предшествующих узлов для каждого уровня.
    unsafe fn find_update(
        &self,
        key: &K,
    ) -> Vec<*mut Node<K, V>> {
        let mut update: Vec<*mut Node<K, V>> = vec![std::ptr::null_mut(); MAX_LEVEL];
        let mut current = self.head.as_ref() as *const Node<K, V> as *mut Node<K, V>;
        for i in (0..self.level).rev() {
            while let Some(next) = (&(*current).forward)[i] {
                if (*next.as_ptr()).key < *key {
                    current = next.as_ptr();
                } else {
                    break;
                }
            }
            update[i] = current;
            debug_assert!(!update[i].is_null(), "update[{i}] must not be null");
        }
        update
    }

    /// Вставляет ключ и значение в пропускной список.
    pub fn insert(
        &mut self,
        key: K,
        value: V,
    ) {
        unsafe {
            let mut update = self.find_update(&key);
            // Проверяем наличие узла с тем же ключом в уровне 0.
            if let Some(node_ptr) = (&(*update[0]).forward)[0] {
                if (*node_ptr.as_ptr()).key == key {
                    (*node_ptr.as_ptr()).value = value;
                    return;
                }
            }
            let new_level = Self::random_level();
            let head_ptr = self.head.as_mut() as *mut Node<K, V>;
            if new_level > self.level {
                update
                    .iter_mut()
                    .take(new_level)
                    .skip(self.level)
                    .for_each(|slot| {
                        *slot = head_ptr;
                    });
                self.level = new_level;
            }
            let new_node = Node::new(key, value, new_level);
            let new_node_ptr = NonNull::new(Box::into_raw(new_node)).unwrap();
            // Обновляем forward-ссылки для уровней от 0 до new_level-1.
            update
                .iter()
                .enumerate()
                .take(new_level)
                .for_each(|(i, &prev)| {
                    (&mut (*new_node_ptr.as_ptr()).forward)[i] = (&(*prev).forward)[i];
                    (&mut (*prev).forward)[i] = Some(new_node_ptr);
                });

            // Устанавливаем backward-ссылку для нового узла (уровень 0).
            // update[0] всегда указывает на узел перед позицией вставки.
            (*new_node_ptr.as_ptr()).backward = Some(NonNull::new_unchecked(update[0]));
            // Если новый узел не последний, обновляем backward следующего узла.
            if let Some(next_ptr) = (&(*new_node_ptr.as_ptr()).forward)[0] {
                (*next_ptr.as_ptr()).backward = Some(new_node_ptr);
            }
            self.length += 1;
        }
    }

    /// Ищет узел с заданным ключом и возвращает ссылку на значение, если
    /// найден.
    pub fn search(
        &self,
        key: &K,
    ) -> Option<&V> {
        let mut current = self.head.as_ref();

        unsafe {
            for i in (0..self.level).rev() {
                while let Some(next) = current.forward[i] {
                    let next_ref = next.as_ref();
                    if &next_ref.key < key {
                        current = next_ref;
                    } else {
                        break;
                    }
                }
            }
            if let Some(node_ptr) = current.forward[0] {
                let node_ref = node_ptr.as_ref();
                if &node_ref.key == key {
                    return Some(&node_ref.value);
                }
            }
        }
        None
    }

    /// Ищет ключ и возвращает изменяемую ссылку на его значение, если он
    /// найден.
    pub fn search_mut(
        &mut self,
        key: &K,
    ) -> Option<&mut V> {
        unsafe {
            let update = self.find_update(key);
            if let Some(node_ptr) = (&(*update[0]).forward)[0] {
                let node_ref = node_ptr.as_ptr();
                if (*node_ref).key == *key {
                    return Some(&mut (*node_ref).value);
                }
            }
        }
        None
    }

    /// Удаляет узел с заданным ключом.
    pub fn remove(
        &mut self,
        key: &K,
    ) -> Option<V> {
        unsafe {
            let mut update = self.find_update(key);

            if let Some(node_ptr) = (&(*update[0]).forward)[0] {
                let node_ref = node_ptr.as_ref();
                if &node_ref.key == key {
                    // Сохраняем значение для возврата.
                    let result = node_ref.value.clone();
                    // Обновляем ссылки на всех уровнях.
                    update
                        .iter_mut()
                        .enumerate()
                        .take(self.level)
                        .for_each(|(i, &mut prev)| {
                            if (&(*prev).forward)[i] == Some(node_ptr) {
                                (&mut (*prev).forward)[i] = node_ref.forward[i];
                            }
                        });
                    // Если существует следующий узел на уровне 0,
                    // обновляем его backward-ссылку.
                    if let Some(next_ptr) = node_ref.forward[0] {
                        (*next_ptr.as_ptr()).backward = node_ref.backward;
                    }
                    // Освобождаем память удаляемого узла.
                    drop(Box::from_raw(node_ptr.as_ptr()));
                    // Корректировка текущего уровня.
                    while self.level > 1 && self.head.forward[self.level - 1].is_none() {
                        self.level -= 1;
                    }
                    self.length -= 1;
                    return Some(result);
                }
            }
        }
        None
    }

    /// Возвращает текущее число элементов в списке.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Возвращает итератор по (&K, &V) в порядке возрастания ключа.
    pub fn iter<'a>(&'a self) -> SkipListIter<'a, K, V> {
        SkipListIter {
            current: self.head.forward[0],
            _marker: PhantomData,
        }
    }

    /// Возвращает итератор по элементам в обратном порядке.
    pub fn iter_rev<'a>(&'a self) -> ReverseIter<'a, K, V> {
        ReverseIter {
            current: self.last_node(),
            head: self.head.as_ref() as *const Node<K, V>,
            _marker: PhantomData,
        }
    }

    /// Возвращает итератор по диапазону: от ключа `start` до ключа `end` (не
    /// включая end).
    pub fn range<'a>(
        &'a self,
        start: &K,
        end: &K,
    ) -> RangeIter<'a, K, V> {
        unsafe {
            let mut current = self.head.as_ref();

            for i in (0..self.level).rev() {
                while let Some(next) = current.forward[i] {
                    let next_ref = next.as_ref();
                    if &next_ref.key < start {
                        current = next_ref;
                    } else {
                        break;
                    }
                }
            }
            let start_ptr = current.forward[0];
            RangeIter {
                current: start_ptr,
                end: Some(end.clone()),
                _marker: std::marker::PhantomData,
            }
        }
    }

    /// Проверяет, содержится ли ключ в списке.
    pub fn contains(
        &self,
        key: &K,
    ) -> bool {
        self.search(key).is_some()
    }

    /// Проверяет на пустоту.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Удаляет все элементы из списка
    pub fn clear(&mut self) {
        unsafe {
            let mut current = self.head.forward[0];
            while let Some(node_ptr) = current {
                current = node_ptr.as_ref().forward[0];
                drop(Box::from_raw(node_ptr.as_ptr()));
            }
            for slot in &mut self.head.forward {
                *slot = None;
            }
            self.level = 1;
            self.length = 0;
        }
    }

    /// Возвращает первый элемент (минимальный ключ) списка.
    pub fn first(&self) -> Option<(&K, &V)> {
        unsafe {
            // Если список пуст, сразу возвращаем None
            self.head.forward[0].map(|node_ptr| {
                let node = node_ptr.as_ref();
                (&node.key, &node.value)
            })
        }
    }

    /// Возвращает последний элемент (максимальный ключ) списка.
    pub fn last(&self) -> Option<(&K, &V)> {
        self.last_node().map(|tail_ptr| {
            let node = unsafe { tail_ptr.as_ref() }; // Только тут и оправдан `unsafe`
            (&node.key, &node.value)
        })
    }

    /// Возвращает указатель на последний элемент (хвост) списка (исключая
    /// голову).
    pub fn last_node(&self) -> Option<NonNull<Node<K, V>>> {
        unsafe {
            let mut current = self.head.as_ref();
            while let Some(next) = current.forward[0] {
                current = next.as_ref();
            }
            // Если current совпадает с head, то список пуст.
            if std::ptr::eq(current, self.head.as_ref()) {
                None
            } else {
                // Преобразуем current совпадает с head, то список пуст.
                NonNull::new(current as *const Node<K, V> as *mut Node<K, V>)
            }
        }
    }

    pub fn validate_invariants(&self) -> Result<(), ValidationError> {
        unsafe {
            let mut count = 0;
            let mut current = self.head.forward[0];

            let mut prev_key: Option<&K> = None;

            while let Some(ptr) = current {
                let node = ptr.as_ref();

                validate!(
                    node.forward.len() <= MAX_LEVEL,
                    ValidationError::InvalidLevel {
                        node_level: node.forward.len(),
                        max_level: MAX_LEVEL
                    }
                );

                if let Some(prev) = prev_key {
                    validate!(
                        prev < &node.key,
                        ValidationError::SortOrderViolation {
                            message: format!("{:?} >= {:?}", prev, node.key)
                        }
                    );
                }

                prev_key = Some(&node.key);

                count += 1;

                current = node.forward[0];
            }

            validate!(
                count == self.length,
                ValidationError::LengthMismatch {
                    expected: self.length,
                    actual: count
                }
            );
        }

        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для SkipList
////////////////////////////////////////////////////////////////////////////////

impl<K, V> Default for SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, K, V> IntoIterator for &'a SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    type Item = (&'a K, &'a V);
    type IntoIter = SkipListIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V> Iterator for SkipListIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            self.current.map(|node_ptr| {
                let node = node_ptr.as_ref();
                self.current = node.forward[0];
                (&node.key, &node.value)
            })
        }
    }
}

impl<'a, K, V> Iterator for ReverseIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if let Some(node_ptr) = self.current {
                // Если достигнут , итерация завершена.
                if std::ptr::eq(node_ptr.as_ptr(), self.head) {
                    None
                } else {
                    let node = node_ptr.as_ref();
                    self.current = node.backward;
                    Some((&node.key, &node.value))
                }
            } else {
                None
            }
        }
    }
}

impl<'a, K, V> Iterator for RangeIter<'a, K, V>
where
    K: Ord + Clone,
    V: Clone,
{
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if let Some(node_ptr) = self.current {
                let node = node_ptr.as_ref();
                if let Some(ref end_key) = self.end {
                    if &node.key >= end_key {
                        return None;
                    }
                }
                self.current = node.forward[0];
                Some((&node.key, &node.value))
            } else {
                None
            }
        }
    }
}

impl<K, V> Serialize for SkipList<K, V>
where
    K: Serialize + Ord + Clone + Default + Debug,
    V: Serialize + Clone + Default + Debug,
{
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let vec: Vec<(K, V)> = self.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        vec.serialize(serializer)
    }
}

impl<'de, K, V> Deserialize<'de> for SkipList<K, V>
where
    K: Deserialize<'de> + Ord + Clone + Default + Debug,
    V: Deserialize<'de> + Clone + Default + Debug,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<(K, V)> = Vec::deserialize(deserializer)?;
        let mut list = SkipList::new();
        for (k, v) in vec {
            list.insert(k, v);
        }
        Ok(list)
    }
}

impl<K, V> Drop for SkipList<K, V> {
    fn drop(&mut self) {
        // Безопасно обходим, начиная с первого элемента.
        unsafe {
            let mut current = self.head.forward[0];
            while let Some(node_ptr) = current {
                // Переходим к следующему узлу до освобождения текущего
                current = node_ptr.as_ref().forward[0];
                // Восстанавливать владение над узлом и освобождаем его память
                drop(Box::from_raw(node_ptr.as_ptr()));
            }
            // В качестве меры на всякий случай очищаем все ссылки в head.
            for slot in self.head.forward.iter_mut() {
                *slot = None;
            }
        }
    }
}

impl<K, V> PartialEq for SkipList<K, V>
where
    K: PartialEq + Ord + Clone + Default + Debug,
    V: PartialEq + Clone + Debug + Default,
{
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        if self.len() != other.len() {
            return false;
        }

        let mut a = self.head.forward[0];
        let mut b = other.head.forward[0];

        unsafe {
            loop {
                match (a, b) {
                    (Some(a_ptr), Some(b_ptr)) => {
                        let a_node = a_ptr.as_ref();
                        let b_node = b_ptr.as_ref();

                        if a_node.key != b_node.key || a_node.value != b_node.value {
                            return false;
                        }

                        a = a_node.forward[0];
                        b = b_node.forward[0];
                    }
                    (None, None) => return true,
                    _ => return false,
                }
            }
        }
    }
}

impl<K, V> Clone for SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    fn clone(&self) -> Self {
        let mut new = SkipList::new();
        for (k, v) in self.iter() {
            new.insert(k.clone(), v.clone());
        }
        new
    }
}

unsafe impl<K: Send, V: Send> Send for SkipList<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for SkipList<K, V> {}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn make_list(data: &[(i32, i32)]) -> SkipList<i32, i32> {
        let mut sl = SkipList::new();
        for (k, v) in data {
            sl.insert(*k, *v);
        }
        sl
    }

    #[test]
    fn test_new_and_basic_properties() {
        let sl: SkipList<i32, i32> = SkipList::new();

        assert_eq!(sl.len(), 0);
        assert!(sl.is_empty());
        assert!(sl.first().is_none());
        assert!(sl.last().is_none());
        assert!(!sl.contains(&1));
    }

    #[test]
    fn test_insert_and_search() {
        let mut sl = SkipList::new();

        sl.insert(10, 100);
        sl.insert(20, 200);
        sl.insert(30, 300);

        assert_eq!(sl.len(), 3);

        assert_eq!(sl.search(&10), Some(&100));
        assert_eq!(sl.search(&20), Some(&200));
        assert_eq!(sl.search(&30), Some(&300));
        assert_eq!(sl.search(&40), None);
    }

    #[test]
    fn test_insert_overwrite() {
        let mut sl = SkipList::new();

        sl.insert(10, 100);
        sl.insert(10, 999);

        assert_eq!(sl.len(), 1);
        assert_eq!(sl.search(&10), Some(&999));
    }

    #[test]
    fn test_remove_existing() {
        let mut sl = make_list(&[(1, 10), (2, 20), (3, 30)]);

        assert_eq!(sl.remove(&2), Some(20));
        assert_eq!(sl.len(), 2);

        assert_eq!(sl.search(&2), None);
        assert!(sl.contains(&1));
        assert!(sl.contains(&3));
    }

    #[test]
    fn test_remove_non_existing() {
        let mut sl = make_list(&[(1, 10), (2, 20)]);

        assert_eq!(sl.remove(&3), None);
        assert_eq!(sl.len(), 2);
    }

    #[test]
    fn test_first_last() {
        let sl = make_list(&[(10, 100), (20, 200), (30, 300)]);

        assert_eq!(sl.first(), Some((&10, &100)));
        assert_eq!(sl.last(), Some((&30, &300)));
    }

    #[test]
    fn test_iter_order() {
        let sl = make_list(&[(3, 30), (1, 10), (2, 20)]);

        let collected: Vec<_> = sl.iter().map(|(k, _)| *k).collect();

        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_reverse_iter_order() {
        let sl = make_list(&[(1, 10), (2, 20), (3, 30)]);

        let collected: Vec<_> = sl.iter_rev().map(|(k, _)| *k).collect();

        assert_eq!(collected, vec![3, 2, 1]);
    }

    #[test]
    fn test_range() {
        let sl = make_list(&[(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)]);

        let collected: Vec<_> = sl.range(&2, &5).map(|(k, _)| *k).collect();

        assert_eq!(collected, vec![2, 3, 4]);
    }

    #[test]
    fn test_clear() {
        let mut sl = make_list(&[(1, 10), (2, 20), (3, 30)]);

        sl.clear();

        assert_eq!(sl.len(), 0);
        assert!(sl.is_empty());
        assert!(sl.first().is_none());
    }

    #[test]
    fn test_clone() {
        let sl1 = make_list(&[(1, 10), (2, 20), (3, 30)]);
        let sl2 = sl1.clone();

        assert_eq!(sl1, sl2);
    }

    #[test]
    fn test_partial_eq() {
        let sl1 = make_list(&[(1, 10), (2, 20)]);
        let sl2 = make_list(&[(1, 10), (2, 20)]);
        let sl3 = make_list(&[(1, 10), (3, 30)]);

        assert_eq!(sl1, sl2);
        assert_ne!(sl1, sl3);
    }

    #[test]
    fn test_search_mut() {
        let mut sl = make_list(&[(1, 10), (2, 20)]);

        if let Some(v) = sl.search_mut(&2) {
            *v = 999;
        }

        assert_eq!(sl.search(&2), Some(&999));
    }

    #[test]
    fn test_against_btreemap() {
        let mut sl = SkipList::new();
        let mut map = BTreeMap::new();

        for i in 0..1000 {
            sl.insert(i, i * 10);
            map.insert(i, i * 10);
        }

        for i in 0..1000 {
            assert_eq!(sl.search(&i), map.get(&i));
        }

        let sl_keys: Vec<_> = sl.iter().map(|(k, _)| *k).collect();
        let map_keys: Vec<_> = map.keys().copied().collect();

        assert_eq!(sl_keys, map_keys);
    }
}
