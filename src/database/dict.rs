//! Хеш-таблица (Dict) с инкрементным рехешированием.
//!
//! Реализация словаря (ассоциативного массива), основанного на хеш-таблице с
//! цепочками, двумя таблицами и плавным рехешированием без пауз.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};

/// Параметры хеш-таблицы
const INITIAL_SIZE: usize = 4;
const REHASH_BATCH: usize = 1;

/// Один элемент в цепочке коллизий.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
struct Entry<K, V> {
    key: K,
    val: V,
    next: Option<Box<Entry<K, V>>>,
}

/// Одна хеш-таблица: вектор бакетов, маска размера и количество занятых
/// элементов.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
struct HashTable<K, V> {
    buckets: Vec<Option<Box<Entry<K, V>>>>,
    size_mask: usize,
    used: usize,
}

/// Основной словарь с двумя таблицами для рехеширования.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Dict<K, V> {
    ht: [HashTable<K, V>; 2],
    rehash_idx: isize, // -1 = без рехеширования, иначе индекс в ht[0]
}

/// Итератор по словарю `Dict`.
pub struct DictIter<'a, K, V> {
    tables: [&'a HashTable<K, V>; 2],
    table_idx: usize,
    bucket_idx: usize,
    current_entry: Option<&'a Entry<K, V>>,
}

impl<K, V> Entry<K, V> {
    /// Создаёт новый элемент цепочки.
    fn new(
        key: K,
        val: V,
        next: Option<Box<Entry<K, V>>>,
    ) -> Box<Self> {
        Box::new(Entry { key, val, next })
    }
}

impl<K, V> HashTable<K, V> {
    /// Создаёт таблицу ёмкостью `cap` (округляется вверх до ближайшей степени
    /// двойки, минимум INITIAL_SIZE).
    fn with_capacity(cap: usize) -> Self {
        let sz = cap.next_power_of_two().max(INITIAL_SIZE);
        let mut buckets = Vec::with_capacity(sz);
        buckets.resize_with(sz, || None);

        HashTable {
            buckets,
            size_mask: sz - 1,
            used: 0,
        }
    }

    /// Сбрасывает таблицу в пустое состояние.
    fn clear(&mut self) {
        self.buckets.clear();
        self.size_mask = 0;
        self.used = 0;
    }
}

impl<K, V> Default for Dict<K, V>
where
    K: Hash + Eq,
{
    /// Создаёт словарь по умолчанию.
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Dict<K, V>
where
    K: Hash + Eq,
{
    /// Создаёт новый пустой словарь.
    pub fn new() -> Self {
        Dict {
            ht: [HashTable::with_capacity(0), HashTable::with_capacity(0)],
            rehash_idx: -1,
        }
    }

    /// Вставляет (key, val). Если ключ уже существует — обновляет значение и
    /// возвращает false.
    pub fn insert(
        &mut self,
        key: K,
        val: V,
    ) -> bool {
        self.expand_if_needed();
        self.rehash_step();

        let table_idx = if self.is_rehashing() { 1 } else { 0 };
        let mask = self.ht[table_idx].size_mask;
        let slot = (Self::hash_key(&key) as usize) & mask;

        // проверка существования ключа
        {
            let mut cur = &mut self.ht[table_idx].buckets[slot];
            while let Some(ref mut e) = cur {
                if e.key == key {
                    e.val = val;
                    return false;
                }
                cur = &mut e.next;
            }
        }

        // вставка нового элемента в начало цепочки
        let next = self.ht[table_idx].buckets[slot].take();
        let new_entry = Entry::new(key, val, next);
        self.ht[table_idx].buckets[slot] = Some(new_entry);
        self.ht[table_idx].used += 1;
        true
    }

    /// Возвращает `&V` для указанного ключа или None.
    pub fn get(
        &mut self,
        key: &K,
    ) -> Option<&V> {
        if self.is_rehashing() {
            self.rehash_step();
        }
        for table_idx in 0..=1 {
            if self.ht[table_idx].size_mask == 0 {
                continue;
            }
            let slot = (Self::hash_key(key) as usize) & self.ht[table_idx].size_mask;
            let mut cur = &self.ht[table_idx].buckets[slot];
            while let Some(ref e) = cur {
                if &e.key == key {
                    return Some(&e.val);
                }
                cur = &e.next;
            }
            if !self.is_rehashing() {
                break;
            }
        }
        None
    }

    /// Удаляет ключ. Возвращает true, если удаление произошло.
    pub fn remove(
        &mut self,
        key: &K,
    ) -> bool {
        if self.is_rehashing() {
            self.rehash_step();
        }

        for table_idx in 0..=1 {
            let table = &mut self.ht[table_idx];
            if table.size_mask == 0 {
                continue;
            }

            let slot = (Self::hash_key(key) as usize) & table.size_mask;

            let old_chain = std::mem::take(&mut table.buckets[slot]);
            let (new_chain, removed) = Self::remove_from_chain(old_chain, key);
            table.buckets[slot] = new_chain;

            if removed {
                table.used -= 1;
                return true;
            }
            if !self.is_rehashing() {
                break;
            }
        }

        false
    }

    /// Возвращает общее количество элементов во всех таблицах.
    pub fn len(&self) -> usize {
        let mut total = self.ht[0].used;
        if self.is_rehashing() {
            total += self.ht[1].used;
        }
        total
    }

    /// Возвращает `true`, если словарь пуст.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Очищает словарь и сбрасывает рехешинг.
    pub fn clear(&mut self) {
        self.ht[0].clear();
        self.ht[1].clear();
        self.rehash_idx = -1;
    }

    /// Возвращает итератор по словарю.
    pub fn iter<'a>(&'a self) -> DictIter<'a, K, V> {
        DictIter {
            tables: [&self.ht[0], &self.ht[1]],
            table_idx: 0,
            bucket_idx: 0,
            current_entry: None,
        }
    }

    /// Рекурсивно обрабатывает цепочку: удаляет первый узел с ключом `key`.
    /// Возвращает (новая_цепочка, был_удалён).
    fn remove_from_chain(
        chain: Option<Box<Entry<K, V>>>,
        key: &K,
    ) -> (Option<Box<Entry<K, V>>>, bool) {
        match chain {
            None => (None, false),
            Some(mut boxed) => {
                if &boxed.key == key {
                    // found — skip this node
                    (boxed.next.take(), true)
                } else {
                    // recurse on tail
                    let (next_chain, removed) = Self::remove_from_chain(boxed.next.take(), key);
                    boxed.next = next_chain;
                    (Some(boxed), removed)
                }
            }
        }
    }

    /// Возвращает true, если в процессе рехеширования.
    #[inline]
    fn is_rehashing(&self) -> bool {
        self.rehash_idx != -1
    }

    /// Вычисляет хеш ключа как u64.
    fn hash_key(key: &K) -> u64 {
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        h.finish()
    }

    /// Выполняет один шаг инкрементного рехеширования.
    fn rehash_step(&mut self) {
        if !self.is_rehashing() {
            return;
        }
        for _ in 0..REHASH_BATCH {
            let idx = self.rehash_idx as usize;
            if idx >= self.ht[0].buckets.len() {
                self.ht[0] = std::mem::replace(&mut self.ht[1], HashTable::with_capacity(0));
                self.rehash_idx = -1;
                return;
            }
            let mut entry_opt = self.ht[0].buckets[idx].take();
            while let Some(mut e) = entry_opt {
                entry_opt = e.next.take();
                let h = (Self::hash_key(&e.key) as usize) & self.ht[1].size_mask;
                e.next = self.ht[1].buckets[h].take();
                self.ht[1].buckets[h] = Some(e);
                self.ht[0].used -= 1;
                self.ht[1].used += 1;
            }
            self.rehash_idx += 1;
        }
    }

    /// Запускает или расширяет рехеширование, если load factor превышает 1.
    fn expand_if_needed(&mut self) {
        if self.is_rehashing() {
            return;
        }
        let used = self.ht[0].used;
        let size = self.ht[0].buckets.len();
        if used >= size {
            // начинаем реhash с новой таблицей вдвое больше
            self.ht[1] = HashTable::with_capacity(size * 2);
            self.rehash_idx = 0;
        }
    }
}

impl<'a, K, V> Iterator for DictIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.current_entry.take() {
                self.current_entry = entry.next.as_deref();
                return Some((&entry.key, &entry.val));
            }

            if self.bucket_idx >= self.tables[self.table_idx].buckets.len() {
                if self.table_idx == 0 && self.tables[1].size_mask != 0 {
                    self.table_idx = 1;
                    self.bucket_idx = 0;
                    continue;
                } else {
                    return None;
                }
            }

            self.current_entry = self.tables[self.table_idx].buckets[self.bucket_idx].as_deref();
            self.bucket_idx += 1;
        }
    }
}

impl<'a, K, V> IntoIterator for &'a Dict<K, V>
where
    K: Hash + Eq,
{
    type Item = (&'a K, &'a V);
    type IntoIter = DictIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет базовые операции вставки и получения значений по ключу.
    #[test]
    fn basic_insert_get() {
        let mut d = Dict::new();
        assert!(d.insert("a", 1));
        assert!(d.insert("b", 2));
        assert_eq!(d.get(&"a"), Some(&1));
        assert_eq!(d.get(&"b"), Some(&2));
        assert_eq!(d.get(&"c"), None);
        assert!(!d.insert("a", 10));
        assert_eq!(d.get(&"a"), Some(&10));
    }

    /// Тест проверяет обновление значения при повторной вставке с тем же
    /// ключом.
    #[test]
    fn insert_updates_existing_key() {
        let mut d = Dict::new();
        assert!(d.insert("key", 42));
        assert!(!d.insert("key", 100));
        assert_eq!(d.get(&"key"), Some(&100));
    }

    /// Тест проверяет удаление: значение удаляется, повторное удаление
    /// возвращает false.
    #[test]
    fn removal() {
        let mut d = Dict::new();
        d.insert("x", 100);
        assert_eq!(d.get(&"x"), Some(&100));
        assert!(d.remove(&"x"));
        assert_eq!(d.get(&"x"), None);
        assert!(!d.remove(&"x"));
    }

    /// Тест проверяет корректную работу словаря при большом количестве вставок
    /// и доступе к значениям.
    #[test]
    fn rehash_behavior() {
        let mut d = Dict::new();
        for i in 0..100 {
            d.insert(i, i * 10);
        }
        for i in 0..100 {
            assert_eq!(d.get(&i), Some(&(i * 10)));
        }
        assert_eq!(d.len(), 100);
    }

    /// Тест проверяет корректное удаление ключей во время рехеширования.
    #[test]
    fn rehash_with_removal() {
        let mut d = Dict::new();
        for i in 0..20 {
            d.insert(i, i);
        }

        for i in 0..10 {
            assert!(d.remove(&i));
        }

        for i in 0..10 {
            assert_eq!(d.get(&i), None);
        }

        for i in 10..20 {
            assert_eq!(d.get(&i), Some(&i));
        }
    }

    /// Тест проверяет, что словарь корректно очищается.
    #[test]
    fn clear_dict() {
        let mut d = Dict::new();
        d.insert("k", "v");
        d.clear();
        assert_eq!(d.len(), 0);
        assert_eq!(d.get(&"k"), None);
    }

    /// Тест проверяет, что словарь можно повторно использовать после очистки.
    #[test]
    fn clear_and_reuse() {
        let mut d = Dict::new();
        d.insert("a", 1);
        d.clear();
        assert_eq!(d.len(), 0);
        assert!(d.insert("a", 2));
        assert_eq!(d.get(&"a"), Some(&2));
    }

    /// Тест проверяет корректную работу итератора по словарю.
    #[test]
    fn iteration_work() {
        let mut d = Dict::new();
        d.insert("x", 1);
        d.insert("y", 2);
        d.insert("z", 3);

        let mut seen = vec![];
        for (k, v) in d.iter() {
            seen.push((k, *v));
        }

        seen.sort();
        assert_eq!(seen, vec![(&"x", 1), (&"y", 2), (&"z", 3)]);
    }

    /// Тест проверяет, что итератор по пустому словарю не возвращает элементов.
    #[test]
    fn empty_iterator() {
        let d: Dict<&str, i32> = Dict::new();
        let mut iter = d.iter();
        assert_eq!(iter.next(), None);
    }
}
