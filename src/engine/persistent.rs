use std::sync::Arc;

#[derive(Clone)]
pub struct PersistentStore {
    pub file_path: Arc<String>,
}

impl PersistentStore {
    pub fn new(file_path: String) -> Self {
        Self {
            file_path: Arc::new(file_path),
        }
    }
}
