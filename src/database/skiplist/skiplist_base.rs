use std::{
    cell::RefCell,
    cmp::PartialEq,
    fmt::Debug,
    rc::{Rc, Weak},
};

use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{SkipListStatistics, ValidationError};

/// Максимальный уровень пропускного списка.
const MAX_LEVEL: usize = 16;
/// Вероятностный коэффициент для определения уровня нового узла.
const P: f64 = 0.5;

type Link<K, V> = Option<Rc<RefCell<Node<K, V>>>>;

/// Узел пропускного списка.
#[derive(Debug)]
pub struct Node<K, V> {
    key: K,
    value: V,
    forward: Vec<Link<K, V>>,
    backward: Weak<RefCell<Node<K, V>>>,
}

/// SkipList — вероятностная структура данных с логарифмическим временем
/// операций.
#[derive(Debug)]
pub struct SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    head: Rc<RefCell<Node<K, V>>>,
    level: usize,
    length: usize,
}

/// Итератор по SkipList на уровне forward[0]
pub struct SkipListIter<'a, K, V> {
    current: Option<Rc<RefCell<Node<K, V>>>>,
    _marker: std::marker::PhantomData<&'a ()>,
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
    ) -> Self {
        Node {
            key,
            value,
            forward: vec![None; level],
            backward: Weak::new(),
        }
    }

    /// Возвращает ссылку на ключ.
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Возвращает ссылку на значение.
    pub fn value(&self) -> &V {
        &self.value
    }

    /// Возвращает изменяемую ссылку на значение.
    pub fn value_mut(&mut self) -> &mut V {
        &mut self.value
    }
}

impl<K, V> SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    /// Создаёт новый пустой SkipList.
    pub fn new() -> Self {
        let head = Rc::new(RefCell::new(Node::new(
            Default::default(),
            Default::default(),
            MAX_LEVEL,
        )));

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

    /// Находит путь обновления для вставки или удаления ключа.
    fn find_update(
        &self,
        key: &K,
    ) -> Vec<Rc<RefCell<Node<K, V>>>> {
        let mut update: Vec<Rc<RefCell<Node<K, V>>>> = Vec::with_capacity(MAX_LEVEL);
        let mut current = Rc::clone(&self.head);

        for i in (0..self.level).rev() {
            loop {
                let next = {
                    let current_node = current.borrow();
                    current_node.forward[i].clone()
                };

                match next {
                    Some(next_rc) => {
                        let next_node = next_rc.borrow();
                        if next_node.key < *key {
                            drop(next_node);
                            current = next_rc;
                        } else {
                            break;
                        }
                    }
                    None => break,
                }
            }

            update.push(Rc::clone(&current));
        }

        // Reverse to get [level-1, level-2, ..., 0]
        update.reverse();

        // Pad with head references for unused levels
        while update.len() < MAX_LEVEL {
            update.push(Rc::clone(&self.head));
        }

        update
    }

    /// Вставляет ключ и значение в пропускной список.
    pub fn insert(
        &mut self,
        key: K,
        value: V,
    ) {
        let update = self.find_update(&key);

        // Проверяем, существует ли узел с таким ключом
        let existing = {
            let prev = update[0].borrow();
            prev.forward[0].clone()
        };

        if let Some(existing_rc) = existing {
            let mut existing_node = existing_rc.borrow_mut();
            if existing_node.key == key {
                // Обновляем существующее значение
                existing_node.value = value;
                return;
            }
        }

        // Генерируем уровень для нового узла
        let new_level = Self::random_level();

        // Обновляем максимальный уровень списка, если необходимо
        if new_level > self.level {
            self.level = new_level;
        }

        // Создаём новый узел
        let new_node = Rc::new(RefCell::new(Node::new(key, value, new_level)));

        // Обновляем forward-ссылки
        for (i, prev_rc) in update.iter().enumerate().take(new_level) {
            let next = {
                let mut prev = prev_rc.borrow_mut();
                let next = prev.forward[i].take();
                prev.forward[i] = Some(Rc::clone(&new_node));
                next
            };

            new_node.borrow_mut().forward[i] = next;
        }

        // Обновляем backward-ссылку для нового узла
        new_node.borrow_mut().backward = Rc::downgrade(&update[0]);

        // Если есть следующий узел, обновляем его backward-ссылку
        if let Some(next) = &new_node.borrow().forward[0] {
            next.borrow_mut().backward = Rc::downgrade(&new_node);
        }

        self.length += 1;

        // Debug-time валидация инвариантов
        #[cfg(debug_assertions)]
        self.validate_invariants()
            .expect("Invariant violation after insert");
    }

    /// Ищет узел с заданным ключом и возвращает ссылку на значение, если
    /// найден.
    pub fn search(
        &self,
        key: &K,
    ) -> Option<V>
    where
        V: Clone,
    {
        let mut current = Rc::clone(&self.head);

        for i in (0..self.level).rev() {
            loop {
                let next = {
                    let current_node = current.borrow();
                    current_node.forward[i].clone()
                };

                match next {
                    Some(next_rc) => {
                        let next_node = next_rc.borrow();

                        if next_node.key < *key {
                            drop(next_node);
                            current = next_rc;
                        } else {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }

        // Проверяем узел на уровне 0
        let possible_match = {
            let current_node = current.borrow();
            current_node.forward[0].clone()
        };

        match possible_match {
            Some(node_rc) => {
                let node = node_rc.borrow();

                if node.key == *key {
                    Some(node.value.clone())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    /// Удаляет узел с заданным ключом.
    pub fn remove(
        &mut self,
        key: &K,
    ) -> Option<V>
    where
        V: Clone,
    {
        let update = self.find_update(key);

        // Проверяем, существует ли узел для удаления
        let to_remove = {
            let prev = update[0].borrow();
            prev.forward[0].clone()
        };

        let to_remove_rc = match to_remove {
            Some(node_rc) => {
                let node = node_rc.borrow();
                if node.key == *key {
                    drop(node);
                    node_rc
                } else {
                    return None;
                }
            }
            None => return None,
        };

        // Извлекаем значение для возврата
        let result = to_remove_rc.borrow().value.clone();

        // Обновляем forward-ссылки
        for (i, prev_rc) in update.iter().enumerate().take(self.level) {
            let mut prev = prev_rc.borrow_mut();

            if let Some(ref node) = prev.forward[i] {
                if Rc::ptr_eq(node, &to_remove_rc) {
                    let next = to_remove_rc.borrow().forward[i].clone();
                    prev.forward[i] = next;
                }
            }
        }

        // Обновляем backward-ссылку следующего узла
        if let Some(next) = &to_remove_rc.borrow().forward[0] {
            next.borrow_mut().backward = to_remove_rc.borrow().backward.clone();
        }

        // Корректируем максимальный уровень
        while self.level > 1 {
            let head_forward = self.head.borrow().forward[self.level - 1].clone();
            if head_forward.is_none() {
                self.level -= 1;
            } else {
                break;
            }
        }

        self.length -= 1;

        // Debug-time валидация
        #[cfg(debug_assertions)]
        self.validate_invariants()
            .expect("Invariant violation after remove");

        Some(result)
    }

    /// Возвращает текущее число элементов в списке.
    pub fn len(&self) -> usize {
        self.length
    }

    ///  Проверяет, пуст ли список.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Проверяет, содержится ли ключ в списке.
    pub fn contains(
        &self,
        key: &K,
    ) -> bool {
        self.search(key).is_some()
    }

    /// Удаляет все элементы из списка
    pub fn clear(&mut self) {
        // Просто обнуляем forward-ссылки головы
        // Rc автоматически освободит память
        for i in 0..MAX_LEVEL {
            self.head.borrow_mut().forward[i] = None;
        }

        self.level = 1;
        self.length = 0;
    }

    /// Возвращает первый элемент (минимальный ключ) списка.
    pub fn first(&self) -> Option<(K, V)>
    where
        K: Clone,
        V: Clone,
    {
        self.head.borrow().forward[0].as_ref().map(|node| {
            let n = node.borrow();
            (n.key.clone(), n.value.clone())
        })
    }

    /// Возвращает последний элемент (максимальный ключ) списка.
    pub fn last(&self) -> Option<(K, V)>
    where
        K: Clone,
        V: Clone,
    {
        let mut current = Rc::clone(&self.head);

        // Идём до конца на нулевом уровне
        loop {
            let next = {
                let current_node = current.borrow();
                current_node.forward[0].clone()
            };

            match next {
                Some(next_rc) => current = next_rc,
                None => break,
            }
        }

        // Если current == head, список пуст
        if Rc::ptr_eq(&current, &self.head) {
            None
        } else {
            let node = current.borrow();
            Some((node.key.clone(), node.value.clone()))
        }
    }

    pub fn validate_invariants(&self) -> Result<(), ValidationError> {
        // Проверка уровня
        if self.level == 0 || self.level > MAX_LEVEL {
            return Err(ValidationError::InvalidLevel {
                node_level: self.level,
                max_level: MAX_LEVEL,
            });
        }

        // Проверяем head
        let head_forward_len = self.head.borrow().forward.len();

        if head_forward_len != MAX_LEVEL {
            return Err(ValidationError::ForwardVectorMismatch {
                expected: MAX_LEVEL,
                actual: head_forward_len,
            });
        }

        // Подсчёт реального кол-ва узлов
        let mut count = 0;
        let mut current = self.head.borrow().forward[0].clone();
        let mut prev_key: Option<K> = None;

        while let Some(node_rc) = current {
            let node = node_rc.borrow();
            count += 1;

            // Проверяем порядок сортировки
            if let Some(ref pk) = prev_key {
                if node.key <= *pk {
                    return Err(ValidationError::SortOrderViolation {
                        message: "Keys not in ascending order".to_string(),
                    });
                }
            }

            prev_key = Some(node.key.clone());

            // Проверяем размер forward вектора
            if node.forward.is_empty() || node.forward.len() > MAX_LEVEL {
                return Err(ValidationError::ForwardVectorMismatch {
                    expected: MAX_LEVEL,
                    actual: node.forward.len(),
                });
            }

            current = node.forward[0].clone();
        }

        // Проверяем длины
        if count != self.length {
            return Err(ValidationError::LengthMismatch {
                expected: self.length,
                actual: count,
            });
        }

        Ok(())
    }

    /// Возвращает итератор по элементам в порядке возрастания ключей
    pub fn iter(&self) -> SkipListIter<'_, K, V> {
        SkipListIter {
            current: self.head.borrow().forward[0].clone(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Собирает статистику о структуре списка.
    pub fn statistics(&self) -> SkipListStatistics {
        let mut stats = SkipListStatistics::empty(MAX_LEVEL);

        stats.node_count = self.length;
        stats.current_max_level = self.level;

        let mut current = self.head.borrow().forward[0].clone();

        while let Some(node_rc) = current {
            let node = node_rc.borrow();
            let node_level = node.forward.len();

            if node_level > 0 && node_level <= MAX_LEVEL {
                stats.level_distribution[node_level - 1] += 1;
            }

            current = node.forward[0].clone();
        }

        stats.compute_average_level();

        stats
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

impl<K, V> Drop for SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    fn drop(&mut self) {
        self.clear()
    }
}

impl<K, V> Serialize for SkipList<K, V>
where
    K: Serialize + Ord + Clone + Default + Debug,
    V: Serialize + Clone + Debug + Default,
{
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let vec: Vec<(K, V)> = {
            let mut vec = Vec::new();
            let mut current = self.head.borrow().forward[0].clone();
            while let Some(node_rc) = current {
                let node = node_rc.borrow();
                vec.push((node.key.clone(), node.value.clone()));
                current = node.forward[0].clone();
            }
            vec
        };
        vec.serialize(serializer)
    }
}

impl<'de, K, V> Deserialize<'de> for SkipList<K, V>
where
    K: Deserialize<'de> + Ord + Clone + Default + Debug,
    V: Deserialize<'de> + Clone + Debug + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<(K, V)>::deserialize(deserializer)?;
        let mut sl = SkipList::new();
        for (k, v) in vec {
            sl.insert(k, v);
        }
        Ok(sl)
    }
}

// Сравнение по-элементно: проверяем len, затем перебираем нулевой уровень.
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

        // начинаем с первого реального узла (forward[0])
        let mut a = self.head.borrow().forward[0].clone();
        let mut b = other.head.borrow().forward[0].clone();

        loop {
            match (a.take(), b.take()) {
                (Some(a_rc), Some(b_rc)) => {
                    let a_node = a_rc.borrow();
                    let b_node = b_rc.borrow();

                    if a_node.key != b_node.key || a_node.value != b_node.value {
                        return false;
                    }

                    a = a_node.forward[0].clone();
                    b = b_node.forward[0].clone();
                }
                (None, None) => return true,
                _ => return false, // длины равны, но структура отличная — защитный кейс
            }
        }
    }
}

impl<K, V> Eq for SkipList<K, V>
where
    K: PartialEq + Ord + Clone + Default + Debug,
    V: PartialEq + Clone + Debug + Default,
{
}

impl<K, V> Clone for SkipList<K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    fn clone(&self) -> Self {
        let mut new_list = SkipList::new();
        let mut current = self.head.borrow().forward[0].clone();

        while let Some(node_rc) = current {
            let node = node_rc.borrow();
            new_list.insert(node.key.clone(), node.value.clone());
            current = node.forward[0].clone();
        }

        new_list
    }
}

impl<'a, K, V> Iterator for SkipListIter<'a, K, V>
where
    K: Ord + Clone + Default + Debug,
    V: Clone + Debug + Default,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node_rc) = self.current.take() {
            // take() заменяет self.current на None и возвращает Option
            let node = node_rc.borrow();
            let item = (node.key.clone(), node.value.clone());
            // клонируем ссылку на следующий узел *до* выхода borrow
            self.current = node.forward[0].clone();
            Some(item)
        } else {
            None
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_skiplist() {
        let list: SkipList<i32, String> = SkipList::new();

        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
        assert_eq!(list.level, 1);
    }

    #[test]
    fn test_insert_and_search() {
        let mut list = SkipList::new();
        list.insert(1, "one");
        list.insert(2, "two");
        list.insert(3, "three");

        assert_eq!(list.search(&1), Some("one"));
        assert_eq!(list.search(&2), Some("two"));
        assert_eq!(list.search(&3), Some("three"));
        assert_eq!(list.search(&4), None);
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_insert_duplicate_updates() {
        let mut list = SkipList::new();

        list.insert(1, "one");
        list.insert(1, "ONE");

        assert_eq!(list.search(&1), Some("ONE"));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_remove() {
        let mut list = SkipList::new();

        list.insert(1, "one");
        list.insert(2, "two");
        list.insert(3, "three");

        assert_eq!(list.remove(&2), Some("two"));
        assert_eq!(list.len(), 2);
        assert_eq!(list.search(&2), None);
        assert_eq!(list.search(&1), Some("one"));
        assert_eq!(list.search(&3), Some("three"));
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut list = SkipList::new();

        list.insert(1, "one");

        assert_eq!(list.remove(&2), None);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut list = SkipList::new();

        list.insert(1, "one");
        list.insert(2, "two");

        list.clear();

        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
        assert_eq!(list.search(&1), None);
    }

    #[test]
    fn test_first_and_last() {
        let mut list = SkipList::new();

        assert_eq!(list.first(), None);
        assert_eq!(list.last(), None);

        list.insert(2, "two");
        list.insert(1, "one");
        list.insert(3, "three");

        assert_eq!(list.first(), Some((1, "one")));
        assert_eq!(list.last(), Some((3, "three")));
    }

    #[test]
    fn test_contains() {
        let mut list = SkipList::new();

        list.insert(1, "one");

        assert!(list.contains(&1));
        assert!(!list.contains(&2));
    }

    #[test]
    fn test_validate_invariants() {
        let mut list = SkipList::new();

        list.insert(1, "one");
        list.insert(2, "two");
        list.insert(3, "three");

        assert!(list.validate_invariants().is_ok());
    }

    #[test]
    fn test_statistics() {
        let mut list = SkipList::new();

        for i in 0..100 {
            list.insert(i, format!("value_{i}"));
        }

        let stats = list.statistics();

        assert_eq!(stats.node_count, 100);
        assert!(stats.average_level > 1.0);
        assert!(stats.average_level < 3.0); // Ожидаемо для P = 0.5
    }

    #[test]
    fn test_large_dataset() {
        let mut list = SkipList::new();
        let n = 1000;

        // Вставка
        for i in 0..n {
            list.insert(i, i * 2);
        }

        assert_eq!(list.len(), n);

        // Поиск
        for i in 0..n {
            assert_eq!(list.search(&i), Some(i * 2));
        }

        // Удаление чётных
        for i in (0..n).step_by(2) {
            assert!(list.remove(&i).is_some());
        }

        assert_eq!(list.len(), n / 2);

        // Проверка оставшихся
        for i in (0..n).step_by(2) {
            assert_eq!(list.search(&i), None);
        }
        for i in (1..n).step_by(2) {
            assert_eq!(list.search(&i), Some(i * 2));
        }
    }
}
