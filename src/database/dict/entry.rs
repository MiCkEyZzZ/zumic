use std::{
    hash::{BuildHasher, Hash, RandomState},
    marker::PhantomData,
};

use crate::database::DictNode;

pub enum Entry<'a, K, V, S = RandomState> {
    Occupied(OccupiedEntry<'a, K, V>),
    Vacant(VacantEntry<'a, K, V, S>),
}

pub struct OccupiedEntry<'a, K, V> {
    pub(crate) slot: &'a mut Option<Box<DictNode<K, V>>>,
    pub(crate) used: &'a mut usize,
}

pub struct VacantEntry<'a, K, V, S = RandomState> {
    pub(crate) key: K,
    pub(crate) slot: &'a mut Option<Box<DictNode<K, V>>>,
    pub(crate) used: &'a mut usize,
    pub(crate) _marker: PhantomData<S>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<'a, K, V> OccupiedEntry<'a, K, V> {
    #[inline]
    pub fn key(&self) -> &K {
        &self.slot.as_ref().unwrap().key
    }

    #[inline]
    pub fn get(&self) -> &V {
        &self.slot.as_ref().unwrap().val
    }

    #[inline]
    pub fn get_mut(&mut self) -> &mut V {
        &mut self.slot.as_mut().unwrap().val
    }

    #[inline]
    pub fn into_mut(self) -> &'a mut V {
        &mut self.slot.as_mut().unwrap().val
    }

    #[inline]
    pub fn insert(
        &mut self,
        val: V,
    ) -> V {
        std::mem::replace(&mut self.slot.as_mut().unwrap().val, val)
    }

    #[inline]
    pub fn remove(self) -> V {
        let mut node = self.slot.take().unwrap();

        *self.slot = node.next.take();
        *self.used -= 1;
        node.val
    }
}

impl<'a, K, V, S> VacantEntry<'a, K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    #[inline]
    pub fn key(&self) -> &K {
        &self.key
    }

    #[inline]
    pub fn into_key(self) -> K {
        self.key
    }

    pub fn insert(
        self,
        val: V,
    ) -> &'a mut V {
        let old_head = self.slot.take();

        *self.slot = Some(Box::new(DictNode {
            key: self.key,
            val,
            next: old_head,
        }));

        *self.used += 1;
        &mut self.slot.as_mut().unwrap().val
    }
}

impl<'a, K, V, S> Entry<'a, K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
    V: Default,
{
    #[inline]
    pub fn or_default(self) -> &'a mut V {
        self.or_insert_with(V::default)
    }
}

impl<'a, K, V, S> Entry<'a, K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    pub fn or_insert(
        self,
        default: V,
    ) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(default),
        }
    }

    pub fn or_insert_with(
        self,
        f: impl FnOnce() -> V,
    ) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(f()),
        }
    }

    pub fn or_insert_with_key(
        self,
        f: impl FnOnce(&K) -> V,
    ) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let val = f(&e.key);
                e.insert(val)
            }
        }
    }

    pub fn and_modify(
        self,
        f: impl FnOnce(&mut V),
    ) -> Self {
        match self {
            Entry::Occupied(mut e) => {
                f(e.get_mut());
                Entry::Occupied(e)
            }
            Entry::Vacant(e) => Entry::Vacant(e),
        }
    }

    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(e) => e.key(),
            Entry::Vacant(e) => e.key(),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dict;

    #[test]
    fn test_entry_or_insert_vacant() {
        let mut d: Dict<&str, u32> = Dict::new();
        let v = d.entry("foo").or_insert(42);

        assert_eq!(*v, 42);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn test_entry_or_insert_occupied() {
        let mut d: Dict<&str, u32> = Dict::new();

        d.insert("foo", 100);

        let v = d.entry("foo").or_insert(999);

        assert_eq!(*v, 100) // не перезаписывает
    }

    #[test]
    fn test_entry_or_insert_with_called_once() {
        let mut d: Dict<&str, Vec<i32>> = Dict::new();
        let mut calls = 0u32;
        d.entry("k").or_insert_with(|| {
            calls += 1;
            vec![1, 2]
        });
        d.entry("k").or_insert_with(|| {
            calls += 1;
            vec![3, 4]
        });
        assert_eq!(calls, 1);
        assert_eq!(d.get(&"k"), Some(&vec![1, 2]));
    }

    #[test]
    fn test_entry_or_insert_with_key() {
        let mut d: Dict<u32, u32> = Dict::new();

        d.entry(7).or_insert_with_key(|&k| k * k);

        assert_eq!(d.get(&7), Some(&49));

        // ф-я повторно не вызывается
        d.entry(7).or_insert_with_key(|_| 0);

        assert_eq!(d.get(&7), Some(&49));
    }

    #[test]
    fn test_entry_or_default_vacant() {
        let mut d: Dict<u32, Vec<u32>> = Dict::new();
        let v = d.entry(5).or_default();

        assert!(v.is_empty());
    }

    #[test]
    fn test_entry_or_default_occupied() {
        let mut d: Dict<u32, u32> = Dict::new();

        d.insert(1, 99);

        let v = d.entry(1).or_default();

        assert_eq!(*v, 99);
    }

    #[test]
    fn test_entry_and_modify_vacant() {
        let mut d: Dict<&str, u32> = Dict::new();

        d.entry("n").and_modify(|v| *v *= 10).or_insert(1);

        assert_eq!(d.get(&"n"), Some(&1)); // and_modify не вызвался
    }

    #[test]
    fn test_entry_word_count() {
        let mut d: Dict<&str, u32> = Dict::new();

        for w in ["a", "b", "a", "c", "a", "b"] {
            d.entry(w).and_modify(|v| *v += 1).or_insert(1);
        }

        assert_eq!(d.get(&"a"), Some(&3));
        assert_eq!(d.get(&"b"), Some(&2));
        assert_eq!(d.get(&"c"), Some(&1));
    }

    #[test]
    fn test_occupied_get_get_mut() {
        let mut d: Dict<&str, u32> = Dict::new();

        d.insert("x", 10);

        if let Entry::Occupied(mut e) = d.entry("x") {
            assert_eq!(*e.get(), 10);
            *e.get_mut() += 5;
            assert_eq!(*e.get(), 15);
        } else {
            panic!("expected Occupied");
        }

        assert_eq!(d.get(&"x"), Some(&15));
    }

    #[test]
    fn test_occupied_into_mut() {
        let mut d: Dict<u32, u32> = Dict::new();

        d.insert(1, 100);

        let r = match d.entry(1) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(_) => panic!("expected Occupied"),
        };

        *r = 999;

        assert_eq!(d.get(&1), Some(&999));
    }

    #[test]
    fn test_occupied_insert_returns_old() {
        let mut d: Dict<&str, String> = Dict::new();

        d.insert("k", "old".into());

        if let Entry::Occupied(mut e) = d.entry("k") {
            let old = e.insert("new".into());

            assert_eq!(old, "old");
            assert_eq!(e.get(), "new");
        } else {
            panic!("expected Occupied");
        }

        assert_eq!(d.get(&"k").map(|s| s.as_str()), Some("new"));
    }

    #[test]
    fn test_occupied_remove() {
        let mut d: Dict<u32, String> = Dict::new();

        d.insert(42, "foo".into());

        if let Entry::Occupied(e) = d.entry(42) {
            let val = e.remove();

            assert_eq!(val, "foo");
        } else {
            panic!("expected Occupied");
        }

        assert_eq!(d.len(), 0);
        assert_eq!(d.get(&42), None);
    }

    #[test]
    fn test_occupied_remove_middle_of_chain() {
        // Принудительно строим коллизию: вставляем несколько элементов и удаляем
        // средний через Entry.
        let mut d: Dict<u32, u32> = Dict::new();

        for i in 0..20u32 {
            d.insert(i, i * 10);
        }

        for i in (0..20u32).step_by(2) {
            if let Entry::Occupied(e) = d.entry(i) {
                e.remove();
            }
        }

        assert_eq!(d.len(), 10);

        for i in (1..20u32).step_by(2) {
            assert_eq!(d.get(&i), Some(&(i * 10)));
        }
    }

    #[test]
    fn test_occupied_key() {
        let mut d: Dict<String, u32> = Dict::new();

        d.insert("foo".into(), 1);

        if let Entry::Occupied(e) = d.entry("foo".into()) {
            assert_eq!(e.key(), "foo");
        } else {
            panic!("expected Occupied");
        }
    }

    #[test]
    fn test_vacant_key_and_into_key() {
        let mut d: Dict<String, u32> = Dict::new();

        if let Entry::Vacant(e) = d.entry("foo".into()) {
            assert_eq!(e.key(), "foo");

            let k = e.into_key();

            assert_eq!(k, "foo");
        } else {
            panic!("expected Vacant");
        }

        assert!(d.is_empty());
    }

    #[test]
    fn test_vacant_insert_returns_mut_ref() {
        let mut d: Dict<u32, Vec<u32>> = Dict::new();

        if let Entry::Vacant(e) = d.entry(1) {
            let v = e.insert(vec![1, 2, 3]);
            v.push(4);
        } else {
            panic!("expected Vacant");
        }

        assert_eq!(d.get(&1), Some(&vec![1, 2, 3, 4]));
    }

    #[test]
    fn test_entry_key_both_variants() {
        let mut d: Dict<u32, u32> = Dict::new();

        d.insert(1, 10);

        assert_eq!(d.entry(1).key(), &1);
        assert_eq!(d.entry(2).key(), &2);
    }

    #[test]
    fn test_entry_during_rehash() {
        let mut d: Dict<u64, u64> = Dict::new();

        for i in 0..20u64 {
            d.insert(i, i);
        }

        // Модифицируем через Entry во время рехеша
        *d.entry(5).or_insert(999) += 100;

        assert_eq!(d.get(&5), Some(&105));

        d.entry(9999).or_insert(42);

        assert_eq!(d.get(&9999), Some(&42));

        // Все старые ключи на месте
        for i in 0..20u64 {
            if i == 5 {
                assert_eq!(d.get(&i), Some(&105));
            } else {
                assert_eq!(d.get(&i), Some(&i));
            }
        }
    }

    #[test]
    fn test_entry_no_duplicate_on_repeated_or_insert() {
        let mut d: Dict<u32, u32> = Dict::new();

        d.insert(1, 10);

        for _ in 0..5 {
            d.entry(1).or_insert(999);
        }

        assert_eq!(d.len(), 1);
        assert_eq!(d.get(&1), Some(&10));
    }

    #[test]
    fn test_entry_remove_then_reinsert() {
        let mut d: Dict<u32, u32> = Dict::new();

        d.insert(1, 10);

        if let Entry::Occupied(e) = d.entry(1) {
            e.remove();
        }

        assert_eq!(d.get(&1), None);

        d.entry(1).or_insert(20);

        assert_eq!(d.get(&1), Some(&20));
    }

    #[test]
    fn test_entry_large_scale() {
        let mut d: Dict<u64, u64> = Dict::new();

        for i in 0..1_000u64 {
            d.entry(i).or_insert(0);
            *d.entry(i).or_insert(0) += 1;
        }

        assert_eq!(d.len(), 1000);

        for i in 0..1_000u64 {
            assert_eq!(d.get(&i), Some(&1));
        }
    }
}
