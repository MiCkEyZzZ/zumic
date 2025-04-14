use std::collections::{BTreeMap, HashMap, HashSet};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use super::{arcbytes::ArcBytes, quicklist::QuickList, SmartHash};

/// Represents a generic value in the storage engine.
///
/// This enum is used as the primary data container for various types
/// supported by the engine. It supports strings, integers, floats,
/// nulls, collections (lists, sets, hashes, sorted sets), as well as
/// more advanced types like HyperLogLog and stream entries.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Value {
    /// Binary-safe string.
    Str(ArcBytes),
    /// Signed 64-bit integer.
    Int(i64),
    /// 64-bit floating-bit point number.
    Float(f64),
    /// Null/None type (used as a placeholder or deletion marker).
    Null,
    /// List of binary strings using a quicklist representation.
    List(QuickList<ArcBytes>),
    /// Hash map (dictionary) stored as SmartHash.
    Hash(SmartHash),
    /// Sorted set implementation with score-based ordering.
    ///
    /// `dict` maps each member to its score,
    /// while `sorted` maintains an ordered map from score to a set of
    /// members.
    ZSet {
        dict: HashMap<ArcBytes, f64>,
        sorted: BTreeMap<OrderedFloat<f64>, HashSet<ArcBytes>>,
    },
    /// Set of unique string elements.
    Set(HashSet<String>),
    /// HyperLogLog structure for approximate cardinality estimation.
    HyperLogLog(HLL),
    /// Stream of entries, each identified by an ID and associated
    /// key-value pairs.
    SStream(Vec<StreamEntry>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamEntry {
    /// Unique identifier of the stream entry.
    pub id: u64,
    /// Map of field names to values.
    pub data: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HLL {
    /// Internal register state used by the HyperLogLog algorithm.
    pub registers: Vec<u8>,
}
