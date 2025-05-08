use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Value;

/// A single entry in a data stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamEntry {
    /// Unique identifier of the stream entry.
    pub id: u64,
    /// Fields and their corresponding values.
    pub data: HashMap<String, Value>,
}
