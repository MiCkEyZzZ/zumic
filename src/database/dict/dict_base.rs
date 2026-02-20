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

/// Минимальное число элементов, которое нужно перенести за один шаг рехеша.
const REHASH_MIN_ENTRIES: usize = 8;

/// Максимальное число пустых бакетов, которое можно пропустить за один шаг.
const REHASH_MAX_EMPTY_VISITS: usize = 64;

/// Load factor, ниже которого запускается автоматический shrink.
const SHRINK_RATIO: f64 = 0.25;

/// Один элемент в цепочке коллизий.
#[derive(PartialEq, Eq, Clone)]
struct Entry<K, V> {
    key: K,
    val: V,
    next: Option<Box<Entry<K, V>>>,
}

/// Одна хеш-таблица: вектор бакетов, маска размера и количество занятых
/// элементов.
#[derive(Debug, PartialEq, Eq, Clone)]
struct HashTable<K, V> {
    buckets: Vec<Option<Box<Entry<K, V>>>>,
    size_mask: usize,
    used: usize,
}

/// Хеш-таблица с инкрементальным рехешированием и параметризуемым хешером.
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

    /// Возвращает ёмкость (кол-во бакетов).
    #[inline]
    fn capacity(&self) -> usize {
        self.buckets.len()
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

    /// Создаёт словарь с предвыделенной ёмкостью и заданным строителем хешеров.
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
        if self.is_rehashing() {
            self.rehash_step();
        }

        let hash = self.make_hash(key);
        let rehashing = self.is_rehashing();
        let (left, right) = self.ht.split_at_mut(1);
        let t0: &mut HashTable<K, V> = &mut left[0];
        let t1: &mut HashTable<K, V> = &mut right[0];

        if !t0.is_empty_table() {
            let slot = (hash as usize) & t0.size_mask;

            if let Some(v) = Self::find_val_mut(&mut t0.buckets[slot], key) {
                return Some(v);
            }
        }

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

                // Проверяем shrink только после реального удаления из ht[0],
                // ВАЖНО! не из ht[1], который временный при рехешировании.
                if table_idx == 0 {
                    self.shrink_if_needed();
                }

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

    /// Возвращает текущую ёмкость (число бакетов) основной таблицы.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.ht[0].capacity()
    }

    /// Очищает словарь и сбрасывает рехешинг.
    pub fn clear(&mut self) {
        self.ht[0].clear();
        self.ht[1].clear();
        self.rehash_idx = -1;
    }

    /// Предвыделяет память для `additional` дополнительных элементов.
    pub fn reserve(
        &mut self,
        additional: usize,
    ) {
        let needed = self.len() + additional;
        let current_cap = self.ht[0].capacity();

        // Если уже достаточно места с учётом load factor <= 1 - ничего не делаем
        if current_cap >= needed {
            return;
        }

        // Форсируем полное рехеширование в таблицу нужного размера.
        self.force_rehash_to(needed);
    }

    /// Уменьшаем выделенную память до минимально необходимой.
    pub fn shrink_to_fit(&mut self) {
        let used = self.len();

        let target = if used == 0 {
            0
        } else {
            used.next_power_of_two().max(INITIAL_SIZE)
        };

        // Уже оптимально
        if self.ht[0].capacity() <= target && !self.is_rehashing() {
            return;
        }

        self.force_rehash_to(target);
    }

    /// Возвращает итератор по парам `(&K, &V)`.
    pub fn iter(&self) -> DictIter<'_, K, V> {
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

    /// Итеративно удаляет первый узел с ключом `key` из цепочки `head` без
    /// рекурсии.
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

    /// Адаптивный шаг инкрементного рехеширования.
    fn rehash_step(&mut self) {
        if !self.is_rehashing() {
            return;
        }

        let mut entries_moved = 0;
        let mut empty_visits = 0;

        while entries_moved < REHASH_MIN_ENTRIES || empty_visits < REHASH_MAX_EMPTY_VISITS {
            let idx = self.rehash_idx as usize;

            // FIX: >= instead of >
            if idx >= self.ht[0].capacity() {
                self.ht[0] = std::mem::replace(&mut self.ht[1], HashTable::with_capacity(0));
                self.rehash_idx = -1;
                return;
            }

            if self.ht[0].buckets[idx].is_none() {
                empty_visits += 1;
                self.rehash_idx += 1;

                if empty_visits >= REHASH_MAX_EMPTY_VISITS && entries_moved > 0 {
                    return;
                }

                continue;
            }

            let mut entry_opt = self.ht[0].buckets[idx].take();

            while let Some(mut e) = entry_opt {
                entry_opt = e.next.take();

                let slot = (self.make_hash(&e.key) as usize) & self.ht[1].size_mask;

                e.next = self.ht[1].buckets[slot].take();
                self.ht[1].buckets[slot] = Some(e);

                self.ht[0].used -= 1; // also important
                self.ht[1].used += 1;

                entries_moved += 1;
            }

            self.rehash_idx += 1;
            empty_visits = 0;
        }
    }

    /// Инициирует рехеширование в увеличенную таблицу, если load factor ≥ 1.
    fn expand_if_needed(&mut self) {
        if self.is_rehashing() {
            return;
        }

        let size = self.ht[0].capacity();
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

    /// Автоматически уменьшает таблицу если load factor ниже [`SHRINK_RATIO`].
    fn shrink_if_needed(&mut self) {
        if self.is_rehashing() {
            return;
        }

        let size = self.ht[0].capacity();
        let used = self.ht[0].used;

        // Защита от слишком маленьких таблиц
        if size <= INITIAL_SIZE {
            return;
        }

        let shrink_threshold = (size as f64 * SHRINK_RATIO) as usize;

        if used < shrink_threshold {
            let new_size = (size / 2).max(INITIAL_SIZE);

            self.ht[1] = HashTable::with_capacity(new_size);
            self.rehash_idx = 0;
        }
    }

    /// Форсирует полное (неинкрементное) рехеширование в таблицу с ёмкостью
    /// `target_cap`.
    fn force_rehash_to(
        &mut self,
        target_cap: usize,
    ) {
        // Сначала завершим текущее рехеширование если оно идёт.
        if self.is_rehashing() {
            self.finish_rehash();
        }

        let new_cap = if target_cap == 0 {
            0
        } else {
            target_cap.next_power_of_two().max(INITIAL_SIZE)
        };

        // Если цель совпадает с текущей — ничего не делаем.
        if self.ht[0].capacity() == new_cap {
            return;
        }

        let mut new_table = HashTable::with_capacity(new_cap);
        let hasher = &self.hasher_builder;

        // Переносим все элементы напрямую — без инкрементности.
        for bucket in &mut self.ht[0].buckets {
            let mut entry_opt = bucket.take();

            while let Some(mut e) = entry_opt {
                entry_opt = e.next.take();

                if new_cap == 0 {
                    // target_cap == 0: просто дропаем элементы (shrink_to_fit на пустом).
                    continue;
                }

                let hash = hasher.hash_one(&e.key);
                let slot = (hash as usize) & new_table.size_mask;
                e.next = new_table.buckets[slot].take();
                new_table.buckets[slot] = Some(e);
                new_table.used += 1;
            }
        }

        self.ht[0] = new_table;
        self.ht[1].clear();
        self.rehash_idx = -1;
    }

    /// Форсированно завершает текущее рехеширование, перенося все оставшиеся
    /// элементы из `ht[0]` в `ht[1]` и финализируя.
    fn finish_rehash(&mut self) {
        if !self.is_rehashing() {
            return;
        }

        let len = self.ht[0].capacity();

        for idx in (self.rehash_idx as usize)..len {
            let mut entry_opt = self.ht[0].buckets[idx].take();

            while let Some(mut e) = entry_opt {
                entry_opt = e.next.take();

                let slot = (self.make_hash(&e.key) as usize) & self.ht[1].size_mask;

                e.next = self.ht[1].buckets[slot].take();

                self.ht[1].buckets[slot] = Some(e);
                self.ht[0].used -= 1;
                self.ht[1].used += 1;
            }
        }

        self.ht[0] = std::mem::replace(&mut self.ht[1], HashTable::with_capacity(0));
        self.rehash_idx = -1;
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
    use rustc_hash::FxBuildHasher;

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
    fn test_with_capacity_preallocates() {
        let mut d: Dict<u32, u32> = Dict::with_capacity(128);
        assert!(!d.is_rehashing());
        for i in 0..64u32 {
            d.insert(i, i);
        }
        for i in 0..64u32 {
            assert_eq!(d.get(&i), Some(&i));
        }
    }

    #[test]
    fn test_with_hasher_fxhash() {
        let mut d: Dict<u64, u64, FxBuildHasher> = Dict::with_hasher(FxBuildHasher);
        for i in 0..100u64 {
            d.insert(i, i * 3);
        }
        for i in 0..100u64 {
            assert_eq!(d.get(&i), Some(&(i * 3)));
        }
        assert_eq!(d.len(), 100);
    }

    #[test]
    fn test_new_uses_ahash_by_default() {
        let mut d: Dict<u32, u32> = Dict::new();
        for i in 0..50 {
            d.insert(i, i * 2);
        }
        for i in 0..50 {
            assert_eq!(d.get(&i), Some(&(i * 2)));
        }
    }

    #[test]
    fn test_with_capacity_and_hasher_fxhash() {
        let mut d: Dict<&str, i32, FxBuildHasher> =
            Dict::with_capacity_and_hasher(64, FxBuildHasher);
        d.insert("one", 1);
        d.insert("two", 2);
        assert_eq!(d.get(&"one"), Some(&1));
        assert_eq!(d.get(&"two"), Some(&2));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn test_two_instances_independent() {
        let mut d1: Dict<u32, u32> = Dict::new();
        let mut d2: Dict<u32, u32> = Dict::new();
        for i in 0..20 {
            d1.insert(i, i);
            d2.insert(i, i * 2);
        }
        for i in 0..20 {
            assert_eq!(d1.get(&i), Some(&i));
            assert_eq!(d2.get(&i), Some(&(i * 2)));
        }
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
    fn test_no_degradation_ahash() {
        let mut d: Dict<u64, u64> = Dict::new();
        const N: u64 = 10_000;
        for i in 0..N {
            d.insert(i, i);
        }
        assert_eq!(d.len() as u64, N);
        for i in 0..N {
            assert_eq!(d.get(&i), Some(&i), "ключ {i} не найден");
        }
    }

    #[test]
    fn test_no_degradation_fxhash() {
        let mut d: Dict<u64, u64, FxBuildHasher> = Dict::with_hasher(FxBuildHasher);
        const N: u64 = 10_000;
        for i in 0..N {
            d.insert(i, i);
        }
        for i in 0..N {
            assert_eq!(d.get(&i), Some(&i));
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

    #[test]
    fn test_serde_roundtrip_ahash() {
        let mut original: Dict<String, i32> = Dict::new();

        for i in 0..50i32 {
            original.insert(format!("key_{i}"), i * 10);
        }

        let json = serde_json::to_string(&original).expect("serialize failed");
        let restored: Dict<String, i32> = serde_json::from_str(&json).expect("deserialize failed");

        assert_eq!(original.len(), restored.len());

        for i in 0..50i32 {
            let k = format!("key_{i}");
            assert_eq!(original.get(&k), restored.get(&k));
        }
    }

    #[test]
    fn test_serde_empty_dict() {
        let empty: Dict<u32, u32> = Dict::new();
        let json = serde_json::to_string(&empty).unwrap();

        assert_eq!(json, "[]");

        let restored: Dict<u32, u32> = serde_json::from_str(&json).unwrap();

        assert!(restored.is_empty());
    }

    #[test]
    fn test_serde_deserialized_dict_is_functional() {
        let mut d: Dict<u32, u32> = Dict::new();

        d.insert(1, 10);
        d.insert(2, 20);

        let json = serde_json::to_string(&d).unwrap();
        let mut restored: Dict<u32, u32> = serde_json::from_str(&json).unwrap();

        restored.insert(3, 30);

        assert_eq!(restored.get(&1), Some(&10));
        assert_eq!(restored.get(&2), Some(&20));
        assert_eq!(restored.get(&3), Some(&30));
    }

    #[test]
    fn test_serde_roundtrip_fxhash() {
        let mut original: Dict<String, u64, FxBuildHasher> = Dict::with_hasher(FxBuildHasher);

        original.insert("alpha".into(), 1);
        original.insert("beta".into(), 2);
        original.insert("gamma".into(), 3);

        let json = serde_json::to_string(&original).unwrap();
        let restored: Dict<String, u64, FxBuildHasher> = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.len(), 3);
        assert_eq!(restored.get(&"alpha".into()), Some(&1));
        assert_eq!(restored.get(&"beta".into()), Some(&2));
        assert_eq!(restored.get(&"gamma".into()), Some(&3));
    }

    #[test]
    fn test_partial_eq_same_content() {
        let mut d1: Dict<u32, u32> = Dict::new();
        let mut d2: Dict<u32, u32> = Dict::new();

        for i in 0..10 {
            d1.insert(i, i);
            d2.insert(i, i);
        }

        assert_eq!(d1, d2);
    }

    #[test]
    fn test_partial_eq_different_content() {
        let mut d1: Dict<u32, u32> = Dict::new();
        let mut d2: Dict<u32, u32> = Dict::new();

        d1.insert(1, 10);
        d2.insert(1, 99);

        assert_ne!(d1, d2);
    }

    #[test]
    fn test_clone_produces_independent_copy() {
        let mut original: Dict<u32, u32> = Dict::new();

        for i in 0..20 {
            original.insert(i, i);
        }

        let mut cloned = original.clone();

        cloned.insert(0, 999);

        assert_eq!(original.get(&0), Some(&0));
        assert_eq!(cloned.get(&0), Some(&999));
    }

    #[test]
    fn test_debug_format() {
        let mut d: Dict<&str, i32> = Dict::new();

        d.insert("x", 42);

        let s = format!("{d:?}");

        assert!(s.contains("x") && s.contains("42"));
    }

    #[test]
    fn test_iter_during_rehash() {
        let mut d = Dict::new();

        for i in 0..100 {
            d.insert(i, i);
        }

        let mut seen = std::collections::HashSet::new();

        for (k, v) in d.iter() {
            assert_eq!(*k, *v);
            seen.insert(*k);
        }

        assert_eq!(seen.len(), 100);
    }
}
