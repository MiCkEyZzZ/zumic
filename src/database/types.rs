use std::collections::{BTreeMap, HashMap, HashSet};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use super::{arcbytes::ArcBytes, quicklist::QuickList};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Value {
    Str(ArcBytes),
    Int(i64),
    Float(f64),
    Null,
    List(QuickList<ArcBytes>),
    Hash(HashMap<ArcBytes, ArcBytes>),
    ZSet {
        dict: HashMap<ArcBytes, f64>,
        sorted: BTreeMap<OrderedFloat<f64>, HashSet<ArcBytes>>,
    },
    Set(HashSet<String>),
    HyperLogLog(HLL),
    SStream(Vec<StreamEntry>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamEntry {
    pub id: u64,
    pub data: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HLL {
    pub registers: Vec<u8>,
}
