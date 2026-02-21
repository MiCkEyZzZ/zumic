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
