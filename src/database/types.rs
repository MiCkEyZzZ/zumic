use std::collections::HashSet;

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use super::{Dict, QuickList, Sds, SkipList, SmartHash, StreamEntry, HLL};
use crate::{StoreError, StoreResult};

/// Represents a generic value in the storage engine.
///
/// This serves as the primary container for various supported data types:
/// strings, integers, floating-point numbers, `null`, collections (lists,
/// sets, hashes, sorted sets), as well as more complex structures like
/// HyperLogLog and streams.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Value {
    /// A binary-safe string.
    Str(Sds),
    /// A 64-bit floating-point number.
    Int(i64),
    /// A 64-bit floating-point number.
    Float(f64),
    /// A `null` type (used to represent absence of value or deletion).
    Null,
    /// A list of binary strings, implemented using `QuickList`.
    List(QuickList<Sds>),
    /// A hash map (dictionary), stored as `SmartHash`.
    Hash(SmartHash),
    /// A sorted set with score-based ordering.
    ///
    /// The `dict` field maps each element to its score,
    /// while `sorted` maintains the order of elements by score.
    ZSet {
        /// Maps each element to its score.
        dict: Dict<Sds, f64>,
        /// Maintains elements ordered by their score.
        sorted: SkipList<OrderedFloat<f64>, Sds>,
    },
    /// A set of unique string elements.
    Set(HashSet<Sds>),
    /// A HyperLogLog structure for approximate cardinality estimation.
    HyperLogLog(HLL),
    /// A stream of entries, each identified by an ID and a set of fields.
    SStream(Vec<StreamEntry>),
}

impl Value {
    /// Serializes the `Value` into JSON bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Value serialization failed")
    }

    /// Deserializes a `Value` from JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> StoreResult<Value> {
        serde_json::from_slice(bytes).map_err(|e| StoreError::SerdeError(e.to_string()))
    }
}
