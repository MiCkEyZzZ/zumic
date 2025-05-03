use std::sync::Arc;

use super::Storage;
use crate::{Sds, StoreError, StoreResult, Value};

pub const SLOT_COUNT: usize = 16384;

#[derive(Clone)]
pub struct ClusterStore {
    pub shards: Vec<Arc<dyn Storage>>,
    pub slots: Vec<usize>, // длина: 16384, каждый слот указывает на индекс в `shards`
}

impl ClusterStore {
    pub fn new(shards: Vec<Arc<dyn Storage>>) -> Self {
        let mut slots = vec![0; SLOT_COUNT];
        for i in 0..SLOT_COUNT {
            slots[i] = i % shards.len();
        }
        Self { shards, slots }
    }

    fn key_slot(key: &Sds) -> usize {
        use crc16::State;
        use crc16::XMODEM;

        let bytes = key.as_bytes();

        // Поиск подстроки между '{' и '}', если есть
        if let Some(start) = bytes.iter().position(|&b| b == b'{') {
            if let Some(end) = bytes[start + 1..].iter().position(|&b| b == b'}') {
                let tag = &bytes[start + 1..start + 1 + end];
                let hash = State::<XMODEM>::calculate(tag);
                return (hash as usize) % SLOT_COUNT;
            }
        }

        // Иначе хешируем всю строку
        let hash = State::<XMODEM>::calculate(bytes);
        (hash as usize) % SLOT_COUNT
    }

    fn get_shard(&self, key: &Sds) -> Arc<dyn Storage> {
        let slot = Self::key_slot(key);
        let shard_idx = self.slots[slot];
        self.shards[shard_idx].clone()
    }
}

impl Storage for ClusterStore {
    fn set(&self, key: Sds, value: Value) -> StoreResult<()> {
        self.get_shard(&key).set(key, value)
    }

    fn get(&self, key: Sds) -> StoreResult<Option<Value>> {
        self.get_shard(&key).get(key)
    }

    fn del(&self, key: Sds) -> StoreResult<i64> {
        self.get_shard(&key).del(key)
    }

    fn mset(&self, entries: Vec<(Sds, Value)>) -> StoreResult<()> {
        for (k, v) in entries {
            self.set(k, v)?;
        }
        Ok(())
    }

    fn mget(&self, keys: &[Sds]) -> StoreResult<Vec<Option<Value>>> {
        keys.iter().map(|key| self.get(key.clone())).collect()
    }

    fn rename(&self, from: Sds, to: Sds) -> StoreResult<()> {
        let from_shard = self.get_shard(&from);
        let to_shard = self.get_shard(&to);
        if !Arc::ptr_eq(&from_shard, &to_shard) {
            return Err(StoreError::WrongShard);
        }
        let val = self.get(from.clone())?.ok_or(StoreError::KeyNotFound)?;
        self.del(from)?;
        self.set(to, val)?;
        Ok(())
    }

    fn renamenx(&self, from: Sds, to: Sds) -> StoreResult<bool> {
        let from_shard = self.get_shard(&from);
        let to_shard = self.get_shard(&to);
        if !Arc::ptr_eq(&from_shard, &to_shard) {
            return Err(StoreError::WrongShard);
        }
        if self.get(to.clone())?.is_some() {
            return Ok(false);
        }
        let val = self.get(from.clone())?.ok_or(StoreError::KeyNotFound)?;
        self.del(from)?;
        self.set(to, val)?;
        Ok(true)
    }

    fn flushdb(&self) -> StoreResult<()> {
        for shard in &self.shards {
            shard.flushdb()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::InMemoryStore;

    use super::*;

    // Helper: для in-memory шарда и кластер над ним.
    fn make_cluster() -> ClusterStore {
        let s1 = Arc::new(InMemoryStore::new());
        let s2 = Arc::new(InMemoryStore::new());
        ClusterStore::new(vec![s1, s2])
    }

    #[test]
    fn test_key_slot_range() {
        let key = Sds::from_str("kin");
        let slot = ClusterStore::key_slot(&key);
        assert!(slot < SLOT_COUNT);
    }

    #[test]
    fn test_set_get_routes_to_correct_shard() {
        let cluster = make_cluster();
        let k1 = Sds::from_str("alpha");
        let v1 = Value::Str(Sds::from_str("A"));

        let k2 = Sds::from_str("beta");
        let v2 = Value::Str(Sds::from_str("B"));

        // Записываем два разных ключа.
        cluster.set(k1.clone(), v1.clone()).unwrap();
        cluster.set(k2.clone(), v2.clone()).unwrap();

        // Проверяем, что get врнёт именно те ключи.
        assert_eq!(cluster.get(k1.clone()).unwrap(), Some(v1));
        assert_eq!(cluster.get(k2.clone()).unwrap(), Some(v2));
    }

    #[test]
    fn test_rename_same_shard_succeeds() {
        let cluster = make_cluster();
        // Используем hash tag "{kin}" и "{kin}dza" — оба в одном слоте
        let from = Sds::from_str("{same}");
        let to = Sds::from_str("{same}new");

        assert_eq!(ClusterStore::key_slot(&from), ClusterStore::key_slot(&to));

        cluster.set(from.clone(), Value::Int(42)).unwrap();
        cluster.rename(from.clone(), to.clone()).unwrap();

        assert_eq!(cluster.get(from).unwrap(), None);
        assert_eq!(cluster.get(to).unwrap(), Some(Value::Int(42)));
    }

    #[test]
    fn test_rename_different_shards_errors() {
        let mut cluster = make_cluster();

        // Принудительно заставим два слота вести на разные шарды
        // Например, 'a' и 'b'
        let a = Sds::from_str("a");
        let b = Sds::from_str("b");

        let slot_a = ClusterStore::key_slot(&a);
        let slot_b = ClusterStore::key_slot(&b);
        cluster.slots[slot_a] = 0;
        cluster.slots[slot_b] = 1;

        cluster.set(a.clone(), Value::Int(7)).unwrap();
        let err = cluster.rename(a, b).unwrap_err();
        assert!(matches!(err, StoreError::WrongShard));
    }

    #[test]
    fn test_renamenx_behaviour() {
        let cluster = make_cluster();

        // Опять же используем hash tags для гарантии одного слота:
        let a = Sds::from_str("{nx}");
        let b = Sds::from_str("{nx}alt");

        assert_eq!(ClusterStore::key_slot(&a), ClusterStore::key_slot(&b));

        cluster.set(a.clone(), Value::Int(1)).unwrap();
        assert_eq!(cluster.renamenx(a.clone(), b.clone()).unwrap(), true);
        assert_eq!(cluster.get(a.clone()).unwrap(), None);
        assert_eq!(cluster.get(b.clone()).unwrap(), Some(Value::Int(1)));
        // Повторный renamenx — на новом ключе уже есть значение
        assert_eq!(cluster.renamenx(a.clone(), b.clone()).unwrap(), false);
    }

    #[test]
    fn test_flushdb_clears_all_shards() {
        let cluster = make_cluster();
        cluster.set(Sds::from_str("one"), Value::Int(1)).unwrap();
        cluster.set(Sds::from_str("two"), Value::Int(2)).unwrap();

        assert!(cluster.get(Sds::from_str("one")).unwrap().is_some());
        assert!(cluster.get(Sds::from_str("two")).unwrap().is_some());

        cluster.flushdb().unwrap();

        assert_eq!(cluster.get(Sds::from_str("one")).unwrap(), None);
        assert_eq!(cluster.get(Sds::from_str("two")).unwrap(), None);
    }

    #[test]
    fn test_key_slot_tag_ignores_outside() {
        let a = Sds::from_str("{tag}");
        let b = Sds::from_str("{tag}kin");
        let c = Sds::from_str("x{tag}kin");
        // Всё, что внутри {tag}, хешируется одинаково
        let sa = ClusterStore::key_slot(&a);
        let sb = ClusterStore::key_slot(&b);
        let sc = ClusterStore::key_slot(&c);
        assert_eq!(sa, sb);
        assert_eq!(sb, sc);
    }
}
