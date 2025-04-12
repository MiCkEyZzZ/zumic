use std::collections::HashMap;

use crate::{
    database::{ArcBytes, QuickList, Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

/// Command for setting a hash field to a specific string value.
/// This command creates or updates the specified field in the hash stored at key.
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
            Some(Value::Hash(mut hash)) => {
                hash.insert(field.clone(), value.clone());
                store.set(key, Value::Hash(hash))?;
                Ok(Value::Int(1))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                let mut hash = HashMap::new();
                hash.insert(field.clone(), value.clone());
                store.set(key, Value::Hash(hash))?;
                Ok(Value::Int(1))
            }
        }
    }
}

/// Command for retrieving the value of a field in a hash.
/// If the field exists, returns the string stored in that field;
/// otherwise, returns a Null value.
#[derive(Debug)]
pub struct HGetCommand {
    pub key: String,
    pub field: String,
}

impl CommandExecute for HGetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let field = ArcBytes::from_str(&self.field);

        if let Some(Value::Hash(ref hash)) = store.get(key.clone())? {
            if let Some(value) = hash.get(&field) {
                // Directly return a clone of the ArcBytes stored in the hash.
                // This avoids converting the bytes to a String and then back to ArcBytes.
                return Ok(Value::Str(value.clone()));
            } else {
                return Ok(Value::Null);
            }
        }
        Ok(Value::Null)
    }
}

/// Command for deleting a field from a hash.
/// Returns 1 if the field was removed, or 0 if the field was not found.
#[derive(Debug)]
pub struct HDelCommand {
    pub key: String,
    pub field: String,
}

impl CommandExecute for HDelCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let field = ArcBytes::from_str(&self.field);

        if let Some(Value::Hash(mut hash)) = store.get(key.clone())? {
            let removed = hash.remove(&field);
            if removed.is_some() {
                store.set(key, Value::Hash(hash))?;
                return Ok(Value::Int(1));
            }
            return Ok(Value::Int(0));
        }
        Ok(Value::Int(0))
    }
}

/// Command for retrieving all fields and their values from a hash.
/// Returns a QuickList of ArcBytes where each element is a formatted string "field: value".
#[derive(Debug)]
pub struct HGetAllCommand {
    pub key: String,
}

impl CommandExecute for HGetAllCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);

        if let Some(Value::Hash(ref hash)) = store.get(key)? {
            // Create a QuickList from the hash entries.
            // Each element is an ArcBytes wrapping a string formatted as "field: value".
            let result: QuickList<ArcBytes> = QuickList::from_iter(
                hash.iter().map(|(k, v)| {
                    // Format the field and value using from_utf8_lossy (which handles any invalid UTF-8 gracefully)
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
