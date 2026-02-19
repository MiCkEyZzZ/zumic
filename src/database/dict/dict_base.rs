use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};

/// Начальный размер таблицы (степень двойки).
const INITIAL_SIZE: usize = 4;

/// Количество бакетов, переносимых за один шаг рехеширования.
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

/// Хеш-таблица с инкрементальным рехешированием.
///
/// **ИНВАРИАНТЫ:**
///
/// - Если `rehash_idx == -1`:
///     - ht[1] пуста
///     - все элементы находятся в ht[0]
///
/// - Если `rehash_idx >= 0`:
///     - рехеширование в процессе
///     - элементы распределены между ht[0] и ht[1]
///
/// - Общее количество элементов всегда равно:
///
///       ht[0].used + ht[1].used
///
/// Рехеширование происходит постепенно во время вставки, удаления или
/// получения изменяемой ссылки на элемент.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Dict<K, V> {
    ht: [HashTable<K, V>; 2],
    rehash_idx: isize,
}

/// Итератор по словарю `Dict` (разделяемая ссылка).
pub struct DictIter<'a, K, V> {
    tables: [&'a HashTable<K, V>; 2],
    table_idx: usize,
    bucket_idx: usize,
    current_entry: Option<&'a Entry<K, V>>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

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
    /// Создаёт таблицу ёмкостью `cap` бакетов.
    fn with_capacity(cap: usize) -> Self {
        if cap == 0 {
            return HashTable {
                buckets: Vec::new(),
                size_mask: 0,
                used: 0,
            };
        }

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

    /// Возвращает `true`, если таблица не инициализирована (нет бакетов).
    #[inline]
    fn is_empty_table(&self) -> bool {
        self.buckets.is_empty()
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

    /// Вставляет пару `(key, val)`.
    pub fn insert(
        &mut self,
        key: K,
        val: V,
    ) -> bool {
        self.expand_if_needed();
        self.rehash_step();

        let hash = Self::hash_key(&key);

        for table_idx in 0..=1 {
            if self.ht[table_idx].is_empty_table() {
                continue;
            }

            let mask = self.ht[table_idx].size_mask;
            let slot = (hash as usize) & mask;
            let mut cur = &mut self.ht[table_idx].buckets[slot];

            while let Some(ref mut e) = cur {
                if e.key == key {
                    e.val = val;
                    return false;
                }

                cur = &mut e.next;
            }

            if !self.is_rehashing() {
                break;
            }
        }

        let table_idx = if self.is_rehashing() { 1 } else { 0 };
        let mask = self.ht[table_idx].size_mask;
        let slot = (hash as usize) & mask;

        // вставка нового элемента в начало цепочки
        let next = self.ht[table_idx].buckets[slot].take();

        self.ht[table_idx].buckets[slot] = Some(Entry::new(key, val, next));
        self.ht[table_idx].used += 1;

        true
    }

    /// Возвращает `Some(&V)` для указанного ключа или `None`.
    pub fn get(
        &self,
        key: &K,
    ) -> Option<&V> {
        let hash = Self::hash_key(key);

        for table_idx in 0..=1 {
            if self.ht[table_idx].is_empty_table() {
                continue;
            }

            let slot = (hash as usize) & self.ht[table_idx].size_mask;
            let mut cur = &self.ht[table_idx].buckets[slot];

            while let Some(ref e) = cur {
                if &e.key == key {
                    return Some(&e.val);
                }

                cur = &e.next;
            }

            // Если рехеширование не идёт — ключ может быть только в ht[0].
            if !self.is_rehashing() {
                break;
            }
        }

        None
    }

    /// Возвращает `Some(&mut V)` для указанного ключа или `None`.
    pub fn get_mut(
        &mut self,
        key: &K,
    ) -> Option<&mut V> {
        if self.is_rehashing() {
            self.rehash_step();
        }

        let hash = Self::hash_key(key);

        for table_idx in 0..=1 {
            if self.ht[table_idx].is_empty_table() {
                continue;
            }

            let slot = (hash as usize) & self.ht[table_idx].size_mask;
            let mut cur = &mut self.ht[table_idx].buckets[slot];

            while let Some(ref mut e) = cur {
                if &e.key == key {
                    // SAFETY: мы немедленно возвращаем ссылку на val, продлевая
                    // время жизни. Borrow checker не позволяет вернуть `&mut e.val`
                    // напрямую из-за промежуточных ссылок, поэтому используем
                    // указатель для явного управления временем жизни.
                    //
                    // Invariant: указатель валиден на всё время жизни `&mut self`,
                    // никаких других изменений структуры не происходит.
                    let val_ptr: *mut V = &mut e.val;

                    return Some(unsafe { &mut *val_ptr });
                }

                cur = &mut e.next;
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

        let hash = Self::hash_key(key);

        for table_idx in 0..=1 {
            let table = &mut self.ht[table_idx];

            if table.is_empty_table() {
                continue;
            }

            let slot = (hash as usize) & table.size_mask;
            let removed = Self::remove_from_chain_iter(&mut table.buckets[slot], key);

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
        self.ht[0].used + self.ht[1].used
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

    /// Возвращает итератор по парам `(&K, &V)`.
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
    fn remove_from_chain_iter(
        head: &mut Option<Box<Entry<K, V>>>,
        key: &K,
    ) -> bool {
        let mut cur = head;
        loop {
            match cur {
                None => return false,
                Some(node) if &node.key == key => {
                    // Изымаем текущий узел, подставляя вместо него его хвост.
                    *cur = node.next.take();
                    return true;
                }
                Some(node) => {
                    cur = &mut node.next;
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
    #[inline]
    fn hash_key(key: &K) -> u64 {
        Self::hash_key_raw(key)
    }

    #[inline]
    fn hash_key_raw<Q: ?Sized + Hash>(key: &Q) -> u64 {
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        h.finish()
    }

    /// Выполняет `REHASH_BATCH` шагов инкрементного рехеширования.
    fn rehash_step(&mut self) {
        if !self.is_rehashing() {
            return;
        }

        for _ in 0..REHASH_BATCH {
            let idx = self.rehash_idx as usize;

            if idx >= self.ht[0].buckets.len() {
                // Все бакеты перенесены - финализируем рехеширование.
                self.ht[0] = std::mem::replace(&mut self.ht[1], HashTable::with_capacity(0));
                self.rehash_idx = -1;
                return;
            }

            // Переносим всю цепочку баета idx из ht[0] в ht[1]
            let mut entry_opt = self.ht[0].buckets[idx].take();

            while let Some(mut e) = entry_opt {
                entry_opt = e.next.take();

                let hash = Self::hash_key(&e.key);
                let slot = (hash as usize) & self.ht[1].size_mask;

                e.next = self.ht[1].buckets[slot].take();

                self.ht[1].buckets[slot] = Some(e);
                self.ht[0].used -= 1;
                self.ht[1].used += 1;
            }

            self.rehash_idx += 1;
        }
    }

    /// Инициирует рехеширование в увеличенную таблицу, если load factor ≥ 1.
    fn expand_if_needed(&mut self) {
        if self.is_rehashing() {
            return;
        }

        let size = self.ht[0].buckets.len();
        let used = self.ht[0].used;

        if size == 0 {
            // Первая вставка: инициализируем ht[0] вместо запуска рехеширования.
            self.ht[0] = HashTable::with_capacity(INITIAL_SIZE);
        } else if used >= size {
            // Load factor ≥ 1: начинаем рехеширование в таблицу вдвое большего размера.
            self.ht[1] = HashTable::with_capacity(size * 2);
            self.rehash_idx = 0;
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для Dict, DictIter
////////////////////////////////////////////////////////////////////////////////

impl<'a, K, V> Iterator for DictIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Отдаём текущий элемент цепочки, если он есть.
            if let Some(entry) = self.current_entry.take() {
                self.current_entry = entry.next.as_deref();
                return Some((&entry.key, &entry.val));
            }

            // Бакеты текущей таблицы исчерпаны.
            if self.bucket_idx >= self.tables[self.table_idx].buckets.len() {
                // Переходим к ht[1], если она непуста (идёт рехеширование).
                if self.table_idx == 0 && !self.tables[1].is_empty_table() {
                    self.table_idx = 1;
                    self.bucket_idx = 0;
                    continue;
                }
                return None;
            }

            // Берём следующий бакет.
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

impl<K, V> Default for Dict<K, V>
where
    K: Hash + Eq,
{
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
