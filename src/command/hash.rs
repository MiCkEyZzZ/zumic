use crate::{
    database::{arcbytes::ArcBytes, quicklist::QuickList, types::Value, SmartHash},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct HSetCommand {
    pub key: String,
    pub field: String,
    pub value: String,
}

impl CommandExecute for HSetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let field = ArcBytes::from_str(&self.field);
        let value = ArcBytes::from_str(&self.value);

        match store.get(key.clone())? {
            Some(Value::Hash(mut smart_hash)) => {
                smart_hash.hset(field.clone(), value.clone());
                store.set(key, Value::Hash(smart_hash))?;
                Ok(Value::Int(1))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                let mut smart_hash = SmartHash::new();
                smart_hash.hset(field.clone(), value.clone());
                store.set(key, Value::Hash(smart_hash))?;
                Ok(Value::Int(1))
            }
        }
    }
}

#[derive(Debug)]
pub struct HGetCommand {
    pub key: String,
    pub field: String,
}

impl CommandExecute for HGetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let field = ArcBytes::from_str(&self.field);

        if let Some(Value::Hash(ref smart_hash)) = store.get(key.clone())? {
            if let Some(value) = smart_hash.hget(&field) {
                return Ok(Value::Str(value.clone()));
            } else {
                return Ok(Value::Null);
            }
        }
        Ok(Value::Null)
    }
}

#[derive(Debug)]
pub struct HDelCommand {
    pub key: String,
    pub field: String,
}

impl CommandExecute for HDelCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let field = ArcBytes::from_str(&self.field);

        if let Some(Value::Hash(mut smart_hash)) = store.get(key.clone())? {
            let removed = smart_hash.hdel(&field);
            if removed {
                store.set(key, Value::Hash(smart_hash))?;
                return Ok(Value::Int(1));
            }
            return Ok(Value::Int(0));
        }
        Ok(Value::Int(0))
    }
}

#[derive(Debug)]
pub struct HGetAllCommand {
    pub key: String,
}

impl CommandExecute for HGetAllCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        if let Some(Value::Hash(ref smart_hash)) = store.get(key)? {
            let result: QuickList<ArcBytes> = QuickList::from_iter(
                smart_hash.iter().map(|(k, v)| {
                    ArcBytes::from(format!(
                        "{}: {}",
                        String::from_utf8_lossy(k),
                        String::from_utf8_lossy(v)
                    ))
                }),
                64, // Set the maximum segment size (adjust if necessary)
            );
            return Ok(Value::List(result));
        }
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::memory::InMemoryStore;

    use super::*;

    fn create_store() -> StorageEngine {
        StorageEngine::InMemory(InMemoryStore::new())
    }

    #[test]
    fn test_hset_and_hget() {
        let mut store = create_store();

        // Set a hash field using HSetCommand.
        let hset_cmd = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };

        let result = hset_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap(), &Value::Int(1));

        // Get the field back.
        let hget_cmd = HGetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
        };

        let get_result = hget_cmd.execute(&mut store);
        assert!(get_result.is_ok());
        assert_eq!(
            get_result.as_ref().unwrap(),
            &Value::Str(ArcBytes::from_str("value1"))
        );
    }

    #[test]
    fn test_hget_nonexistent_field() {
        let mut store = create_store();

        let hset_cmd = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };
        hset_cmd.execute(&mut store).unwrap();

        let hget_cmd = HGetCommand {
            key: "hash".to_string(),
            field: "nonexistent".to_string(),
        };
        let get_result = hget_cmd.execute(&mut store);

        match get_result {
            Ok(Value::Null) => {}
            other => panic!("Expected Ok(Value::Null), got {:?}", other),
        }
    }

    #[test]
    fn test_hdel_command() {
        let mut store = create_store();

        // Set a field in the hash.
        let hset_cmd = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };
        hset_cmd.execute(&mut store).unwrap();

        // Delete the field.
        let hdel_cmd = HDelCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
        };
        let del_result = hdel_cmd.execute(&mut store);
        match del_result {
            Ok(Value::Int(1)) => {}
            other => panic!("Expected Ok(Value::Int(1)), got {:?}", other),
        }

        // Attempt to get the deleted field.
        let hget_cmd = HGetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
        };
        let get_result = hget_cmd.execute(&mut store);
        match get_result {
            Ok(Value::Null) => {}
            other => panic!("Expected Ok(Value::Null), got {:?}", other),
        }
    }

    #[test]
    fn test_hgetall_command() {
        let mut store = create_store();

        // Set two fields in the hash.
        let hset_cmd1 = HSetCommand {
            key: "hash".to_string(),
            field: "field1".to_string(),
            value: "value1".to_string(),
        };
        let hset_cmd2 = HSetCommand {
            key: "hash".to_string(),
            field: "field2".to_string(),
            value: "value2".to_string(),
        };
        hset_cmd1.execute(&mut store).unwrap();
        hset_cmd2.execute(&mut store).unwrap();

        // Use HGetAllCommand to get all fields and values.
        let hgetall_cmd = HGetAllCommand {
            key: "hash".to_string(),
        };
        let result = hgetall_cmd.execute(&mut store).unwrap();

        // The result should be a Value::List containing a QuickList of ArcBytes.
        // Each element should be formatted as "field: value".
        if let Value::List(quicklist) = result {
            // Convert the QuickList into a Vec for easy inspection.
            let items: Vec<String> = quicklist
                .iter()
                .map(|ab| ab.as_str().unwrap_or("").to_string())
                .collect();
            // The order in QuickList is defined by the iteration order of the HashMap,
            // so we sort the results to compare.
            let mut sorted_items = items.clone();
            sorted_items.sort();
            assert_eq!(
                sorted_items,
                vec!["field1: value1".to_string(), "field2: value2".to_string()]
            );
        } else {
            panic!("Expected Value::List from HGetAllCommand");
        }
    }
}
