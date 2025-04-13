use crate::{
    database::{arcbytes::ArcBytes, quicklist::QuickList, types::Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::execute::CommandExecute;

#[derive(Debug)]
pub struct SetCommand {
    pub key: String,
    pub value: Value,
}

impl CommandExecute for SetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.set(ArcBytes::from_str(self.key.as_str()), self.value.clone())?;
        Ok(Value::Null)
    }
}

#[derive(Debug)]
pub struct GetCommand {
    pub key: String,
}

impl CommandExecute for GetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let result = store.get(ArcBytes::from_str(self.key.as_str()));
        match result {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(Value::Null),
            Err(e) => Err(StoreError::from(e)),
        }
    }
}

#[derive(Debug)]
pub struct DelCommand {
    pub key: String,
}

impl CommandExecute for DelCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let deleted = store.del(ArcBytes::from_str(&self.key))?;
        Ok(Value::Int(deleted))
    }
}

#[derive(Debug)]
pub struct ExistsCommand {
    pub keys: Vec<String>,
}

impl CommandExecute for ExistsCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let count = self
            .keys
            .iter()
            .map(|key| ArcBytes::from_str(key))
            .filter_map(|key| store.get(key).ok())
            .filter(|value| value.is_some())
            .count();

        Ok(Value::Int(count as i64))
    }
}

#[derive(Debug)]
pub struct SetNxCommand {
    pub key: String,
    pub value: Value,
}

impl CommandExecute for SetNxCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let exists = store.get(ArcBytes::from_str(&self.key))?.is_some();
        if !exists {
            store.set(ArcBytes::from_str(&self.key), self.value.clone())?;
            Ok(Value::Int(1))
        } else {
            Ok(Value::Int(0))
        }
    }
}

#[derive(Debug)]
pub struct MSetCommand {
    pub entries: Vec<(String, Value)>,
}

impl CommandExecute for MSetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let converted = self
            .entries
            .iter()
            .map(|(k, v)| (ArcBytes::from_str(k), v.clone()))
            .collect();
        store.mset(converted)?;
        Ok(Value::Null)
    }
}

#[derive(Debug)]
pub struct MGetCommand {
    pub keys: Vec<String>,
}

impl CommandExecute for MGetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let converted_keys: Vec<ArcBytes> =
            self.keys.iter().map(|k| ArcBytes::from_str(k)).collect();

        let values = store.mget(&converted_keys)?;

        let vec: Vec<ArcBytes> = values
            .into_iter()
            .map(|opt| match opt {
                Some(Value::Str(s)) => Ok(s),
                Some(_) => Err(StoreError::WrongType("Неверный тип".to_string())),
                None => Ok(ArcBytes::from_str("")), // пустая строка для None
            })
            .collect::<Result<_, _>>()?;

        let mut list = QuickList::new(64);
        for item in vec {
            list.push_back(item);
        }

        Ok(Value::List(list))
    }
}

#[derive(Debug)]
pub struct RenameCommand {
    pub from: String,
    pub to: String,
}

impl CommandExecute for RenameCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.rename(ArcBytes::from_str(&self.from), ArcBytes::from_str(&self.to))?;
        Ok(Value::Str(ArcBytes::from_str("")))
    }
}

#[derive(Debug)]
pub struct RenameNxCommand {
    pub from: String,
    pub to: String,
}

impl CommandExecute for RenameNxCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let success =
            store.renamenx(ArcBytes::from_str(&self.from), ArcBytes::from_str(&self.to))?;
        Ok(Value::Int(if success { 1 } else { 0 }))
    }
}

#[derive(Debug)]
pub struct FlushDbCommand;

impl CommandExecute for FlushDbCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.flushdb()?;
        Ok(Value::Str(ArcBytes::from_str("OK")))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        command::{
            CommandExecute, DelCommand, ExistsCommand, FlushDbCommand, GetCommand, RenameCommand,
            RenameNxCommand, SetNxCommand,
        },
        database::{arcbytes::ArcBytes, types::Value},
        engine::{engine::StorageEngine, memory::InMemoryStore},
    };

    use super::{MGetCommand, MSetCommand, SetCommand};

    /// Testing `SetCommand` and `GetCommand`.
    /// It checks that after setting a value with `SetCommand`,
    /// it can be correctly retrieved with `GetCommand`.
    #[test]
    fn test_set_and_get() {
        // Initialize store
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a SetCommand
        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: crate::database::types::Value::Str(ArcBytes::from_str("test_value")),
        };

        // Execute the set command
        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {:?}", result);

        // Create a GetCommand
        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        // Execute the get command
        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        // Check that the value matches
        assert_eq!(
            result.unwrap(),
            Value::Str(ArcBytes::from_str("test_value"))
        );
    }

    /// Testing `GetCommand` for a non-existing key.
    /// It checks that the command returns `Null` for non-existing keys.
    #[test]
    fn test_get_non_existent_key() {
        // Initialize store
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a GetCommand with non-existent key
        let get_command = GetCommand {
            key: "non_existent_key".to_string(),
        };

        // Execute the get command
        let result = get_command.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        // Check that Null is returned for a non-existent key
        assert_eq!(result.unwrap(), Value::Null);
    }

    /// Testing `DelCommand` for an existing key.
    /// It checks that deleting an existing key returns 1
    /// and the key is actually deleted.
    #[test]
    fn test_del_existing_key() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a command SetCommand
        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: crate::database::types::Value::Str(ArcBytes::from_str("test_value")),
        };

        // Execute the set command
        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {:?}", result);

        // Create a DelCommand command
        let del_cmd = DelCommand {
            key: "test_key".to_string(),
        };

        // Execute the del command
        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {:?}", del_result);

        // Check that 1 is returned (1 value removed)
        assert_eq!(del_result.unwrap(), Value::Int(1));

        // Check that the key no longer exists
        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        // Check that Null is returned for the deleted key
        assert_eq!(result.unwrap(), Value::Null);
    }

    #[test]
    fn test_del_non_existent_key() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a DelCommand with a non-existent key
        let del_cmd = DelCommand {
            key: "non_existent_key".to_string(),
        };

        // Execute the del command
        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {:?}", del_result);

        // Check that 0 is returned (nothing removed)
        assert_eq!(del_result.unwrap(), Value::Int(0));
    }

    /// Testing `ExistsCommand`.
    /// It checks that the command correctly counts the number of existing keys.
    #[test]
    fn test_exists_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Check for existence before adding keys
        let exists_cmd = ExistsCommand {
            keys: vec!["test_key1".to_string(), "test_key2".to_string()],
        };
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(0)); // Both keys are missing

        // Add one of the keys
        let set_cmd = SetCommand {
            key: "test_key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Check again - one key exists
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(1)); // Only one exists

        // Add the second key
        let set_cmd2 = SetCommand {
            key: "test_key2".to_string(),
            value: Value::Str(ArcBytes::from_str("another")),
        };
        set_cmd2.execute(&mut store).unwrap();

        // Now both should exist
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(2)); // Both exist
    }

    /// Testing `ExistsCommand` with an empty list of keys.
    /// It checks that the command correctly returns 0 for an empty list.
    #[test]
    fn test_exists_empty_keys() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        let exists_cmd = ExistsCommand { keys: vec![] };
        let result = exists_cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(0)); // Empty list - zero
    }

    /// Testing `SetNxCommand` for a key that does not exist.
    /// It checks that the command sets the key and returns 1.
    #[test]
    fn test_setnx_key_not_exists() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a SetNxCommand command with a new key
        let setnx_cmd = SetNxCommand {
            key: "new_key".to_string(),
            value: Value::Str(ArcBytes::from_str("new_value")),
        };

        // Execute SETNX command
        let result = setnx_cmd.execute(&mut store);

        // Check that the command returned 1 (the key was installed)
        assert!(result.is_ok(), "SetNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1));

        // Check that the value is actually set
        let get_cmd = GetCommand {
            key: "new_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(
            get_result.unwrap(),
            Value::Str(ArcBytes::from_str("new_value"))
        );
    }

    /// Testing `SetNxCommand` for a key that already exists.
    /// It checks that the command returns 0 and does not overwrite the value.
    #[test]
    fn test_setnx_key_exists() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a SetNxCommand command with an existing key
        let set_cmd = SetNxCommand {
            key: "existing_key".to_string(),
            value: Value::Str(ArcBytes::from_str("value")),
        };

        // Execute SETNX command to set value
        let _ = set_cmd.execute(&mut store);

        // Now we try to perform SETNX for the same key
        let setnx_cmd = SetNxCommand {
            key: "existing_key".to_string(),
            value: Value::Str(ArcBytes::from_str("new_value")),
        };

        // Execute SETNX command for an existing key
        let result = setnx_cmd.execute(&mut store);

        // Check that the command returned 0 (the key already exists)
        assert!(result.is_ok(), "SetNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(0));

        // Check that the value has not changed
        let get_cmd = GetCommand {
            key: "existing_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Str(ArcBytes::from_str("value")));
    }

    /// Testing `MSetCommand` for setting multiple keys.
    /// It checks that the command sets multiple keys and their values correctly.
    #[test]
    fn test_mset() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a command MSetCommand
        let mset_cmd = MSetCommand {
            entries: vec![
                ("key1".to_string(), Value::Str(ArcBytes::from_str("value1"))),
                ("key2".to_string(), Value::Str(ArcBytes::from_str("value2"))),
            ],
        };

        // Execute mset commands
        let result = mset_cmd.execute(&mut store);
        assert!(result.is_ok(), "MSetCommand failed: {:?}", result);

        // Check that the values ​​have been set.
        let get_cmd1 = GetCommand {
            key: "key1".to_string(),
        };

        let get_result1 = get_cmd1.execute(&mut store);
        assert!(get_result1.is_ok(), "GetCommand failed: {:?}", get_result1);
        assert_eq!(
            get_result1.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };

        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {:?}", get_result2);
        assert_eq!(
            get_result2.unwrap(),
            Value::Str(ArcBytes::from_str("value2"))
        );
    }

    /// This test ensures that the `MGetCommand` works correctly. It first sets multiple key-value pairs using `MSetCommand`,
    /// and then retrieves them using the `MGetCommand`. The test verifies that the values returned by the `MGetCommand`
    /// match the expected values for each key in the list. Specifically:
    /// 1. The keys "key1" and "key2" are set with values "value1" and "value2".
    /// 2. The `MGetCommand` retrieves the correct values for these keys.
    #[test]
    fn test_mget() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create MSetCommand for multiple keys
        let mset_cmd = MSetCommand {
            entries: vec![
                ("key1".to_string(), Value::Str(ArcBytes::from_str("value1"))),
                ("key2".to_string(), Value::Str(ArcBytes::from_str("value2"))),
            ],
        };
        mset_cmd.execute(&mut store).unwrap();

        // Create a command MGetCommand
        let mget_cmd = MGetCommand {
            keys: vec!["key1".to_string(), "key2".to_string()],
        };

        // Execute mget command
        let result = mget_cmd.execute(&mut store);
        assert!(result.is_ok(), "MGetCommand failed: {:?}", result);

        // Check that the list returned has the correct values
        let result_list = match result.unwrap() {
            Value::List(list) => list,
            _ => panic!("Expected Value::List, got something else"),
        };

        let values: Vec<String> = result_list
            .into_iter()
            .map(|item| String::from_utf8_lossy(&item).to_string())
            .collect();

        assert_eq!(values, vec!["value1".to_string(), "value2".to_string()]);
    }

    /// This test ensures that the `RenameCommand` works as expected. It renames an existing key to a new key name.
    /// The test first creates a key, executes the rename operation, and verifies that:
    /// 1. The new key exists with the original value.
    /// 2. The old key no longer exists in the store.
    #[test]
    fn test_rename() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a command SetCommand
        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Create a command RenameCommand
        let rename_cmd = RenameCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        // Execute the rename command
        let result = rename_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameCommand failed: {:?}", result);

        // Check if the key has been renamed
        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(
            get_result.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );

        // Check that the old key no longer exists
        let get_cmd_old = GetCommand {
            key: "key1".to_string(),
        };
        let get_result_old = get_cmd_old.execute(&mut store);
        assert!(
            get_result_old.is_ok(),
            "GetCommand failed: {:?}",
            get_result_old
        );
        assert_eq!(get_result_old.unwrap(), Value::Null);
    }

    /// This test ensures that the `RenameNxCommand` works as expected when renaming a key to a new key name.
    /// The `RenameNxCommand` only renames the key if the target key does not already exist.
    /// It first adds a key, executes the rename operation, and verifies that the new key exists with the original value,
    /// and that the old key is deleted successfully.
    #[test]
    fn test_renamenx() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a command SetCommand
        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Create a command RenameNxCommand
        let rename_cmd = RenameNxCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        // Execute renamenx command
        let result = rename_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1)); // Успех

        // Check that the new key exists
        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(
            get_result.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );

        // Check that the old key no longer exists
        let get_cmd_old = GetCommand {
            key: "key1".to_string(),
        };
        let get_result_old = get_cmd_old.execute(&mut store);
        assert!(
            get_result_old.is_ok(),
            "GetCommand failed: {:?}",
            get_result_old
        );
        assert_eq!(get_result_old.unwrap(), Value::Null);
    }

    /// This test ensures that the `RenameNxCommand` works as expected when renaming a key only if the target key does not already exist.
    /// It first adds a key, attempts to rename it with `RenameNxCommand` (where the target key does not exist),
    /// and verifies that the key is successfully renamed. It then checks that the old key no longer exists and the new key is present.
    #[test]
    fn test_rename_nx_key_not_exists() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a command SetCommand
        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Create a command RenameNxCommand
        let rename_nx_cmd = RenameNxCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        // Execute rename command (key "key2" does not exist yet)
        let result = rename_nx_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1)); // Renaming was successful

        // Check that the old key no longer exists
        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Null); // Key deleted

        // Check that the new key exists
        let get_cmd_new = GetCommand {
            key: "key2".to_string(),
        };
        let get_result_new = get_cmd_new.execute(&mut store);
        assert!(
            get_result_new.is_ok(),
            "GetCommand failed: {:?}",
            get_result_new
        );
        assert_eq!(
            get_result_new.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );
    }

    /// This test ensures that the `FlushDbCommand` properly clears all keys from the database.
    /// It first adds two keys, executes the flush command, and then checks that both keys have been removed.
    #[test]
    fn test_flushdb() {
        // Initialize storage
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Add multiple keys
        let set_cmd1 = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd1.execute(&mut store).unwrap();

        let set_cmd2 = SetCommand {
            key: "key2".to_string(),
            value: Value::Str(ArcBytes::from_str("value2")),
        };
        set_cmd2.execute(&mut store).unwrap();

        // Create a FlushDbCommand
        let flush_cmd = FlushDbCommand;

        // Execute flushdb command
        let result = flush_cmd.execute(&mut store);
        assert!(result.is_ok(), "FlushDbCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Str(ArcBytes::from_str("OK")));

        // Check that all keys have been deleted
        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Null); // Key "key1" deleted

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };
        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {:?}", get_result2);
        assert_eq!(get_result2.unwrap(), Value::Null); // Key "key2" removed
    }
}
