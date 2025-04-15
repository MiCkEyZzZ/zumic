use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt::Debug, ptr::NonNull};

/// Максимальный уровень пропускного списка.
const MAX_LEVEL: usize = 16; // В дальнейшем этот параметр можно сделать настраиваемым

/// Вероятностный коэффициент для определения уровня нового узла.
const P: f64 = 0.5;

/// Узел пропускного списка.
/// Каждый узел хранит ключ, значение и вектор указателей на следующие узлы
/// на каждом уровне. Поле forward указывает на следующий узел (или None,если
/// нет дальнейших узлов).
#[derive(Debug, PartialEq, Clone)]
pub struct Node<K, V> {
    pub key: K,
    pub value: V,
    pub forward: Vec<Option<NonNull<Node<K, V>>>>, // Для каждого уровня хранится указатель на следующий узел.
}

/// SkipList - структура, содержащая Head-узла и текущий уровень.
#[derive(Debug, PartialEq, Clone)]
pub struct SkipList<K, V> {
    /// Head пропускного списка. Head не содержит полезных данных, служит только для связей.
    head: Box<Node<K, V>>,
    /// Текущий максимальный уровень.
    level: usize,
    /// Количество элементов (не учитывая головы)
    length: usize,
}

/// Итератор для SkipList по нижнему уровню.
pub struct SkipListIter<'a, K, V> {
    current: Option<NonNull<Node<K, V>>>,
    _marker: std::marker::PhantomData<&'a Node<K, V>>,
}

impl<K, V> Node<K, V> {
    /// Создаёт новый узел с заданным уровнем.
    fn new(key: K, value: V, level: usize) -> Box<Self> {
        // Заполняем вектор уровней None
        Box::new(Node {
            key,
            value,
            forward: vec![None; level],
        })
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
    /// Вставляет ключ и значение в пропускной список.
    /// Если ключ уже существует, обновляет значение.
    pub fn insert(&mut self, key: K, value: V) {
        let mut update: Vec<*mut Node<K, V>> = vec![std::ptr::null_mut(); MAX_LEVEL];
        let mut current = &mut *self.head as *mut Node<K, V>;

        unsafe {
            // Поиск места вставки – идём по уровням начиная с наивысшего.
            for i in (0..self.level).rev() {
                while let Some(next) = (*current).forward[i] {
                    if next.as_ref().key < key {
                        current = next.as_ptr();
                    } else {
                        break;
                    }
                }
                update[i] = current;
                debug_assert!(!update[i].is_null(), "update[{}] must not be null", i);
            }

            // Проверяем существует ли ключ на уровне 0
            if let Some(node_ptr) = (*current).forward[0] {
                let node = node_ptr.as_ref();
                if node.key == key {
                    // Обновляем существуещее значение.
                    (node_ptr.as_ptr() as *mut Node<K, V>)
                        .as_mut()
                        .unwrap()
                        .value = value;
                    return;
                }
            }

            // Иначе создаём новый узел
            let new_level = Self::random_level();
            if new_level > self.level {
                for i in self.level..new_level {
                    update[i] = &mut *self.head;
                }
                self.level = new_level;
            }

            // Создаем новый узел.
            let new_node = Node::new(key, value, new_level);
            let new_node_ptr = NonNull::new(Box::into_raw(new_node)).unwrap();

            // Обновляем forward-ссылки.
            for i in 0..new_level {
                let prev = update[i];
                let prev_ref = &mut *prev;
                (*new_node_ptr.as_ptr()).forward[i] = prev_ref.forward[i];
                prev_ref.forward[i] = Some(new_node_ptr);
            }
            self.length += 1;
        }
    }
    /// Ищет узел с заданным ключом и возвращает ссылку на значение, если найден.
    pub fn search(&self, key: &K) -> Option<&V> {
        let mut current = &*self.head;
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
    /// Ищет ключ и возвращает изменяемую ссылку на его значение, если он найден.
    pub fn search_mut(&mut self, key: &K) -> Option<&mut V> {
        let mut current = &mut *self.head as *mut Node<K, V>;
        unsafe {
            for i in (0..self.level).rev() {
                while let Some(next) = (*current).forward[i] {
                    if next.as_ref().key < *key {
                        current = next.as_ptr();
                    } else {
                        break;
                    }
                }
            }
            if let Some(node_ptr) = (*current).forward[0] {
                let node_ref = node_ptr.as_ptr();
                if (*node_ref).key == *key {
                    return Some(&mut (*node_ref).value);
                }
            }
        }
        None
    }
    /// Удаляет узел с заданным ключом.
    /// Возвращает значение удаленного узла, если он был найден.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let mut update: Vec<*mut Node<K, V>> = vec![std::ptr::null_mut(); MAX_LEVEL];
        let mut current = &mut *self.head as *mut Node<K, V>;

        unsafe {
            for i in (0..self.level).rev() {
                while let Some(next) = (*current).forward[i] {
                    if next.as_ref().key < *key {
                        current = next.as_ptr();
                    } else {
                        break;
                    }
                }
                update[i] = current;
            }

            if let Some(node_ptr) = (*current).forward[0] {
                let node_ref = node_ptr.as_ref();
                if &node_ref.key == key {
                    // Сохраняем значение для возврата.
                    let result = node_ref.value.clone();
                    // Обновляем ссылки на всех уровнях.
                    for i in 0..self.level {
                        if (*update[i]).forward[i] == Some(node_ptr) {
                            (*update[i]).forward[i] = node_ref.forward[i];
                        }
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
    pub fn iter(&self) -> SkipListIter<K, V> {
        SkipListIter {
            current: self.head.forward[0],
            _marker: std::marker::PhantomData,
        }
    }
    // Новый метод, позволяющий получить итератор в обратном порядке
    pub fn iter_rev(&self) -> impl DoubleEndedIterator<Item = (&K, &V)> {
        // Собираем все элементы в вектор
        let mut items: Vec<(&K, &V)> = self.iter().collect();
        // Возвращаем итератор по вектору в обратном порядке
        items.reverse();
        items.into_iter()
    }
    /// Проверяет, содержится ли ключ в списке.
    pub fn contains(&self, key: &K) -> bool {
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
    pub fn front(&self) -> Option<(&K, &V)> {
        unsafe {
            // Если список пуст, сразу возвращаем None
            self.head.forward[0].map(|node_ptr| {
                let node = node_ptr.as_ref();
                (&node.key, &node.value)
            })
        }
    }
    pub fn back(&self) -> Option<(&K, &V)> {
        unsafe {
            let mut current = &*self.head;
            loop {
                match current.forward[0] {
                    Some(next_ptr) => {
                        current = next_ptr.as_ref();
                    }
                    None => break,
                }
            }
            if std::ptr::eq(current, &*self.head) {
                None
            } else {
                Some((&current.key, &current.value))
            }
        }
    }
}

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

impl<K, V> Serialize for SkipList<K, V>
where
    K: Serialize + Ord + Clone + Default + std::fmt::Debug,
    V: Serialize + Clone + Default + std::fmt::Debug,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
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

#[cfg(test)]
mod tests {
    use super::*;

    // Test that inserting and searching work correctly for various keys and values.
    #[test]
    fn test_insert_and_search() {
        let mut list = SkipList::new();
        list.insert(1, "a");
        list.insert(3, "c");
        list.insert(2, "b");

        assert_eq!(list.search(&1), Some(&"a"));
        assert_eq!(list.search(&2), Some(&"b"));
        assert_eq!(list.search(&3), Some(&"c"));
        assert_eq!(list.search(&4), None);
    }

    // Test that inserting the same key twice updates the value.
    #[test]
    fn test_insert_overwrite() {
        let mut list = SkipList::new();
        list.insert(42, "first");
        assert_eq!(list.search(&42), Some(&"first"));
        list.insert(42, "second");
        assert_eq!(list.search(&42), Some(&"second"));
    }

    // Test that removing a key works and updates the list correctly.
    #[test]
    fn test_remove() {
        let mut list = SkipList::new();
        list.insert(10, "x");
        list.insert(20, "y");
        assert_eq!(list.remove(&10), Some("x"));
        assert_eq!(list.search(&10), None);
        assert_eq!(list.remove(&10), None);
        assert_eq!(list.remove(&20), Some("y"));
        assert!(list.is_empty());
    }

    // Test that the length and empty checks are correct.
    #[test]
    fn test_len_and_is_empty() {
        let mut list = SkipList::new();
        assert!(list.is_empty());
        list.insert(1, "a");
        list.insert(2, "b");
        assert_eq!(list.len(), 2);
        list.remove(&1);
        assert_eq!(list.len(), 1);
        list.clear();
        assert!(list.is_empty());
    }

    // Test that iterating over the list returns elements in ascending key order.
    #[test]
    fn test_iter_order() {
        let mut list = SkipList::new();
        list.insert(3, "c");
        list.insert(1, "a");
        list.insert(2, "b");

        let items: Vec<_> = list.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(items, vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    // Test that front and back methods return the first and last elements correctly.
    #[test]
    fn test_front_and_back() {
        let mut list = SkipList::new();
        assert_eq!(list.front(), None);
        assert_eq!(list.back(), None);

        list.insert(10, "x");
        list.insert(5, "y");
        list.insert(30, "z");

        assert_eq!(list.front(), Some((&5, &"y")));
        assert_eq!(list.back(), Some((&30, &"z")));
    }

    // Test that search_mut allows updating values correctly.
    #[test]
    fn test_search_mut() {
        let mut list = SkipList::new();
        list.insert(7, "a");
        if let Some(v) = list.search_mut(&7) {
            *v = "b";
        }
        assert_eq!(list.search(&7), Some(&"b"));
    }

    // Test that the clear method removes all elements from the list.
    #[test]
    fn test_clear() {
        let mut list = SkipList::new();
        list.insert(1, "one");
        list.insert(2, "two");
        list.clear();
        assert!(list.is_empty());
        assert_eq!(list.search(&1), None);
        assert_eq!(list.search(&2), None);
    }
}
