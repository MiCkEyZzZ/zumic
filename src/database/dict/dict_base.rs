use std::{
    fmt::{self, Debug},
    hash::{BuildHasher, Hash},
    marker::PhantomData,
};

use ahash::RandomState;
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};

/// Начальный размер таблицы (степень двойки).
const INITIAL_SIZE: usize = 4;

/// Количество бакетов, переносимых за один шаг рехеширования.
const REHASH_BATCH: usize = 1;

/// Один элемент в цепочке коллизий.
#[derive(PartialEq, Eq, Clone, Serialize, Deserialize)]
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
///     - вторая таблица (`ht[1]`) пуста;
///     - все элементы находятся в первой таблице (`ht[0]`).
///
/// - Если `rehash_idx >= 0`:
///     - рехеширование в процессе;
///     - элементы распределены между первой и второй таблицами (`ht[0]` и
///       `ht[1]`).
///
/// - Общее количество элементов всегда равно сумме элементов обеих таблиц.
///   Например: количество элементов в `ht[0]` плюс количество элементов в
///   `ht[1]`.
///
/// Рехеширование происходит постепенно во время вставки, удаления или
/// получения изменяемой ссылки на элемент.
pub struct Dict<K, V, S = RandomState> {
    ht: [HashTable<K, V>; 2],
    rehash_idx: isize,
    hasher_builder: S,
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

impl<K, V> Dict<K, V, RandomState>
where
    K: Hash + Eq,
{
    /// Создаёт новый пустой словарь с [`ahash::RandomState`].
    pub fn new() -> Self {
        Dict::with_hasher(RandomState::new())
    }

    /// Создаёт словарь с предвыделенной ёмкостью и [`ahash::RandomState`].
    pub fn with_capacity(cap: usize) -> Self {
        Dict::with_capacity_and_hasher(cap, RandomState::new())
    }
}

impl<K, V, S> Dict<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Создаёт пустой словарь с заданным строителем хешеров.
    pub fn with_hasher(hasher_builder: S) -> Self {
        Dict {
            ht: [HashTable::with_capacity(0), HashTable::with_capacity(0)],
            rehash_idx: -1,
            hasher_builder,
        }
    }

    /// Создаёт словарь с предвыделенной ёмкостью и заданным строителями
    /// хешеров.
    pub fn with_capacity_and_hasher(
        cap: usize,
        hasher_builder: S,
    ) -> Self {
        let init = HashTable::with_capacity(if cap == 0 { 0 } else { cap });

        Dict {
            ht: [init, HashTable::with_capacity(0)],
            rehash_idx: -1,
            hasher_builder,
        }
    }
}

impl<K, V, S> Dict<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Вставляет пару `(key, val)`.
    pub fn insert(
        &mut self,
        key: K,
        val: V,
    ) -> bool {
        self.expand_if_needed();
        self.rehash_step();

        let hash = self.make_hash(&key);

        // Поиск существующего ключа (обе таблицы при рехешировании).
        for table_idx in 0..=1 {
            if self.ht[table_idx].is_empty_table() {
                if !self.is_rehashing() {
                    break;
                }

                continue;
            }

            let slot = (hash as usize) & self.ht[table_idx].size_mask;
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

        // Вставка нового элемента в начало цепочки целевой таблицы.
        let table_idx = if self.is_rehashing() { 1 } else { 0 };
        let slot = (hash as usize) & self.ht[table_idx].size_mask;
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
        let hash = self.make_hash(key);

        for table_idx in 0..=1 {
            if self.ht[table_idx].is_empty_table() {
                if !self.is_rehashing() {
                    break;
                }

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
        // Выполним шаг рехеша, если нужно.
        if self.is_rehashing() {
            self.rehash_step();
        }

        let hash = self.make_hash(key);

        // Считаем состояние рехеширования заранее — до того, как мы возьмём
        // mutable borrows частей `self`.
        let rehashing = self.is_rehashing();

        // Возьмём mut ссылки на обе таблицы как отдельные непересекающиеся borrows.
        // split_at_mut(1) даёт [0] и [1] как независимые mutable ссылки.
        let (left, right) = self.ht.split_at_mut(1);
        let t0: &mut HashTable<K, V> = &mut left[0];
        let t1: &mut HashTable<K, V> = &mut right[0];

        // Сначала ищем в ht[0]
        if !t0.is_empty_table() {
            let slot = (hash as usize) & t0.size_mask;
            if let Some(v) = Self::find_val_mut(&mut t0.buckets[slot], key) {
                return Some(v);
            }
        }

        // Только если идёт рехеширование — ищем во второй таблице.
        if rehashing && !t1.is_empty_table() {
            let slot = (hash as usize) & t1.size_mask;
            if let Some(v) = Self::find_val_mut(&mut t1.buckets[slot], key) {
                return Some(v);
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

        let hash = self.make_hash(key);

        for table_idx in 0..=1 {
            let table = &mut self.ht[table_idx];

            if table.is_empty_table() {
                if !self.is_rehashing() {
                    break;
                }

                continue;
            }

            let slot = (hash as usize) & table.size_mask;

            if Self::remove_from_chain_iter(&mut table.buckets[slot], key) {
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
    #[inline]
    pub fn len(&self) -> usize {
        self.ht[0].used + self.ht[1].used
    }

    /// Возвращает `true`, если словарь пуст.
    #[inline]
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

    /// Вычисляет хеш ключа через `self.hasher_builder`.
    #[inline]
    fn make_hash(
        &self,
        key: &K,
    ) -> u64 {
        self.hasher_builder.hash_one(key)
    }

    /// Безопасный мутабельный поиск по цепочке через match-guard cursor.
    fn find_val_mut<'a>(
        mut head: &'a mut Option<Box<Entry<K, V>>>,
        key: &K,
    ) -> Option<&'a mut V> {
        while let Some(ref mut boxed) = head {
            if &boxed.key == key {
                return Some(&mut boxed.val);
            }
            head = &mut boxed.next;
        }
        None
    }

    /// Итеративно удаляет первый узел с ключом `key` из цепочки `head`.
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

                let slot = (self.make_hash(&e.key) as usize) & self.ht[1].size_mask;

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
// Общие реализации трейтов для Dict, DictIter, Entry
////////////////////////////////////////////////////////////////////////////////

impl<K: Debug, V: Debug> Debug for Entry<K, V> {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Entry")
            .field("key", &self.key)
            .field("val", &self.val)
            .finish()
    }
}

impl<K, V> Default for Dict<K, V, RandomState>
where
    K: Hash + Eq,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V, S> Debug for Dict<K, V, S>
where
    K: Hash + Eq + Debug,
    V: Debug,
    S: BuildHasher,
{
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K, V, S> Clone for Dict<K, V, S>
where
    K: Hash + Eq + Clone,
    V: Clone,
    S: BuildHasher + Clone,
{
    fn clone(&self) -> Self {
        Dict {
            ht: [self.ht[0].clone(), self.ht[1].clone()],
            rehash_idx: self.rehash_idx,
            hasher_builder: self.hasher_builder.clone(),
        }
    }
}

impl<K, V, S> PartialEq for Dict<K, V, S>
where
    K: Hash + Eq,
    V: PartialEq,
    S: BuildHasher,
{
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        if self.len() != other.len() {
            return false;
        }

        self.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl<K, V, S> Eq for Dict<K, V, S>
where
    K: Hash + Eq,
    V: Eq,
    S: BuildHasher,
{
}

impl<K, V, S> Serialize for Dict<K, V, S>
where
    K: Hash + Eq + Serialize,
    V: Serialize,
    S: BuildHasher,
{
    fn serialize<Ser>(
        &self,
        serializer: Ser,
    ) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for (k, v) in self.iter() {
            seq.serialize_element(&(k, v))?;
        }
        seq.end()
    }
}

impl<'de, K, V, S> Deserialize<'de> for Dict<K, V, S>
where
    K: Hash + Eq + Deserialize<'de>,
    V: Deserialize<'de>,
    S: BuildHasher + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DictVisitor<K, V, S>(PhantomData<(K, V, S)>);

        impl<'de, K, V, S> Visitor<'de> for DictVisitor<K, V, S>
        where
            K: Hash + Eq + Deserialize<'de>,
            V: Deserialize<'de>,
            S: BuildHasher + Default,
        {
            type Value = Dict<K, V, S>;

            fn expecting(
                &self,
                f: &mut fmt::Formatter,
            ) -> fmt::Result {
                write!(f, "sequence of (key, value) pairs")
            }

            fn visit_seq<A>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut dict = Dict::with_hasher(S::default());
                while let Some((k, v)) = seq.next_element::<(K, V)>()? {
                    dict.insert(k, v);
                }
                Ok(dict)
            }
        }

        deserializer.deserialize_seq(DictVisitor(PhantomData))
    }
}

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

impl<'a, K, V, S> IntoIterator for &'a Dict<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    type Item = (&'a K, &'a V);
    type IntoIter = DictIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_insert_get() {
        let mut d = Dict::new();
        assert!(d.insert("a", 1));
        assert!(d.insert("b", 2));
        assert_eq!(d.get(&"a"), Some(&1));
        assert_eq!(d.get(&"b"), Some(&2));
        assert_eq!(d.get(&"c"), None);
        assert!(!d.insert("a", 10));
        assert_eq!(d.get(&"a"), Some(&10));
    }

    #[test]
    fn test_insert_updates_existing_key() {
        let mut d = Dict::new();
        assert!(d.insert("key", 42));
        assert!(!d.insert("key", 100));
        assert_eq!(d.get(&"key"), Some(&100));
    }

    #[test]
    fn test_removal() {
        let mut d = Dict::new();
        d.insert("x", 100);
        assert_eq!(d.get(&"x"), Some(&100));
        assert!(d.remove(&"x"));
        assert_eq!(d.get(&"x"), None);
        assert!(!d.remove(&"x"));
    }

    #[test]
    fn test_rehash_behavior() {
        let mut d = Dict::new();
        for i in 0..100 {
            d.insert(i, i * 10);
        }
        for i in 0..100 {
            assert_eq!(d.get(&i), Some(&(i * 10)));
        }
        assert_eq!(d.len(), 100);
    }

    #[test]
    fn test_rehash_with_removal() {
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

    #[test]
    fn test_clear_dict() {
        let mut d = Dict::new();
        d.insert("k", "v");
        d.clear();
        assert_eq!(d.len(), 0);
        assert_eq!(d.get(&"k"), None);
    }

    #[test]
    fn test_clear_and_reuse() {
        let mut d = Dict::new();
        d.insert("a", 1);
        d.clear();
        assert_eq!(d.len(), 0);
        assert!(d.insert("a", 2));
        assert_eq!(d.get(&"a"), Some(&2));
    }

    #[test]
    fn test_iteration_work() {
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

    #[test]
    fn test_empty_iterator() {
        let d: Dict<&str, i32> = Dict::new();
        let mut iter = d.iter();
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_get_does_not_require_mut() {
        let mut d = Dict::new();

        d.insert("a", 1u32);
        d.insert("b", 2u32);

        // Оба get работают на иммутабельной ссылке
        let d_ref: &Dict<_, _> = &d;
        let va = d_ref.get(&"a");
        let vb = d_ref.get(&"b");

        assert_eq!(va, Some(&1));
        assert_eq!(vb, Some(&2));
    }

    #[test]
    fn test_get_mut_modifies_in_place() {
        let mut d = Dict::new();

        d.insert("counter", 0u64);

        *d.get_mut(&"counter").unwrap() += 10;
        *d.get_mut(&"counter").unwrap() += 5;

        assert_eq!(d.get(&"counter"), Some(&15));
    }

    #[test]
    fn test_mut_missing_key() {
        let mut d: Dict<&str, i32> = Dict::new();

        assert!(d.get_mut(&"ghost").is_none());
    }

    #[test]
    fn test_first_insert_into_empty_table() {
        let mut d = Dict::new();

        // Внутренние таблицы изначально пусты.
        assert!(d.ht[0].is_empty_table());
        assert!(d.ht[1].is_empty_table());
        assert!(!d.is_rehashing());

        d.insert("key", 42);

        // После первой вставки ht[0] должен быть инициализирован
        assert!(!d.ht[0].is_empty_table());
        assert_eq!(d.ht[0].used, 1);
        assert_eq!(d.get(&"key"), Some(&42));
        assert!(!d.is_rehashing());
    }

    #[test]
    fn test_insert_then_remove_single() {
        let mut d = Dict::new();

        assert!(d.insert("only", 99));
        assert_eq!(d.len(), 1);
        assert!(d.remove(&"only"));
        assert_eq!(d.len(), 0);
        assert!(d.is_empty());
        assert_eq!(d.get(&"only"), None);
    }

    #[test]
    fn test_remove_from_empty_dict() {
        let mut d: Dict<&str, i32> = Dict::new();
        assert!(!d.remove(&"missing"));
    }

    #[test]
    fn test_long_chain_no_stack_overflow() {
        let mut d = Dict::new();

        for i in 0..10_000u64 {
            d.insert(i, i);
        }

        // удаляем все элементы - итеративный `remove_from_chain_iter`
        // не должен паниковать из-за стека.
        for i in 0..10_000u64 {
            assert!(d.remove(&i));
        }

        assert!(d.is_empty());
    }

    #[test]
    fn test_repeated_insert_remove_cycles() {
        let mut d = Dict::new();

        for cycle in 0..10u32 {
            for i in 0..50u32 {
                d.insert(i, cycle * 50 + i);
            }

            for i in 0..50u32 {
                assert_eq!(d.get(&i), Some(&(cycle * 50 + i)));
            }

            for i in 0..50u32 {
                assert!(d.remove(&i));
            }

            assert!(d.is_empty());
        }
    }

    #[test]
    fn test_get_on_new_dict() {
        let d: Dict<u32, u32> = Dict::new();

        assert_eq!(d.get(&0), None);
        assert_eq!(d.get(&42), None);
    }

    #[test]
    fn test_len_during_rehash() {
        let mut d = Dict::new();

        // Вставляем достаточно элементов для начала рехеширования.
        for i in 0..8u32 {
            d.insert(i, i);
        }

        // Вне зависимости от стадии рехеширования len должен быть точным.
        assert_eq!(d.len(), 8);
    }

    #[test]
    fn test_get_mut_visible_via_get() {
        let mut d = Dict::new();

        d.insert("score", 100i32);

        if let Some(v) = d.get_mut(&"score") {
            *v += 50;
        }

        assert_eq!(d.get(&"score"), Some(&150));
    }

    #[test]
    fn test_iter_single_element() {
        let mut d = Dict::new();

        d.insert("solo", 7u8);

        let pairs: Vec<_> = d.iter().collect();

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (&"solo", &7u8));
    }
}
