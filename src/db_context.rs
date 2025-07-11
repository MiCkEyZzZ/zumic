use crate::{InMemoryStore, Sds, StorageEngine, StoreResult, Value};

pub struct DbContext {
    engine: StorageEngine,
}

impl DbContext {
    /// Создаёт новый контекст на основе движка памяти.
    pub fn new_inmemory() -> Self {
        Self {
            engine: StorageEngine::Memory(InMemoryStore::new()),
        }
    }
    /// Делегирует `SET key value`.
    pub fn set(
        &mut self,
        key: Sds,
        value: Value,
    ) -> StoreResult<()> {
        self.engine.set(&key, value)
    }
    /// Делегируем `GET key`.
    pub fn get(
        &self,
        key: Sds,
    ) -> StoreResult<Option<Value>> {
        self.engine.get(&key)
    }
    /// Делегируем `DEL key`.
    pub fn del(
        &mut self,
        key: Sds,
    ) -> StoreResult<bool> {
        self.engine.del(&key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    #[test]
    fn test_set_get_del() {
        let mut ctx = DbContext::new_inmemory();

        let key = Sds::from(b"foo" as &[u8]);
        let val = Value::Str(Sds::from(b"bar" as &[u8]));

        let set_result = ctx.set(key.clone(), val.clone());
        assert!(set_result.is_ok());

        let get_result = ctx.get(key.clone());
        assert!(get_result.is_ok());
        assert_eq!(get_result.unwrap(), Some(val.clone()));

        let del_result = ctx.del(key.clone());
        assert!(del_result.is_ok());
        assert!(del_result.unwrap());

        let get_after_del = ctx.get(key.clone());
        assert!(get_after_del.is_ok());
        assert_eq!(get_after_del.unwrap(), None);
    }
}
