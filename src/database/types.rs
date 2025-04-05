use std::collections::{BTreeMap, HashMap, HashSet};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use super::{arcbytes::ArcBytes, quicklist::QuickList};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Value {
    Str(ArcBytes),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    List(QuickList<ArcBytes>),
    ZSet {
        dict: HashMap<ArcBytes, f64>,
        sorted: BTreeMap<OrderedFloat<f64>, HashSet<ArcBytes>>,
    },
    Hash(HashMap<ArcBytes, ArcBytes>),
    Set(HashSet<String>),
}

/*

pub enum Value {
    Str(ArcBytes),
    Int(i64),
    Float(f64),
    List(QuickList<Value>),
    Hash(HashMap<String, Value>),
    ZSet(BTreeMap<OrderedFloat<f64>, String>),
    Set(HashSet<String>),
    HyperLogLog(HLL),
    SStream(Vec<StreamEntry>),
}
*/
