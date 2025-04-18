use std::sync::Arc;

use super::Storage;

#[derive(Clone)]
pub struct ClusterStore {
    pub shards: Vec<Arc<dyn Storage>>, // Список shards
}

impl ClusterStore {
    pub fn new(shards: Vec<Arc<dyn Storage>>) -> Self {
        Self { shards }
    }
}
