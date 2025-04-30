use std::sync::Arc;

use crate::StoragePort;

#[derive(Clone)]
pub struct ClusterStore {
    pub shards: Vec<Arc<dyn StoragePort>>, // Список shards
}

impl ClusterStore {
    pub fn new(shards: Vec<Arc<dyn StoragePort>>) -> Self {
        Self { shards }
    }
}
