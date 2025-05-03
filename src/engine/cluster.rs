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
        use crc16::*;
        let bytes = key.as_bytes();
        State::<XMODEM>::calculate(bytes) as usize % SLOT_COUNT
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
        let val = self.get(from.clone())?.ok_or(StoreError::KeyNotFound)?;
        self.del(from)?;
        self.set(to, val)?;
        Ok(())
    }

    fn renamenx(&self, from: Sds, to: Sds) -> StoreResult<bool> {
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
