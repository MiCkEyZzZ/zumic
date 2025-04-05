use std::{
    fs::{File, OpenOptions},
    io::{Read, Result, Write},
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use lru::LruCache;

use super::storage::Storage;
use crate::database::types::Value;

/// `WriteAheadLog`: Журнал предзаписи (WAL), который записывает операции перед их выполнением.
pub struct WriteAheadLog {
    file: File,
}

/// `Snapshotter`: Механизм создания снапшотов.
pub struct Snapshotter {
    path: String,
}

/// `PersistentStore`: Хранилище с WAL, снапшотами и LRU-кэшем.
pub struct PersistentStore {
    pub wal: Arc<Mutex<WriteAheadLog>>,
    pub snapshot: Arc<Snapshotter>,
    pub cache: Mutex<LruCache<String, Value>>,
}

impl PersistentStore {
    pub fn new(wal_path: &str, snapshot_path: &str, cache_size: usize) -> Result<Self> {
        Ok(Self {
            wal: Arc::new(Mutex::new(WriteAheadLog::new(wal_path)?)),
            snapshot: Arc::new(Snapshotter::new(snapshot_path)),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(cache_size).unwrap())),
        })
    }
}

impl Storage for PersistentStore {
    fn set(&mut self, key: String, value: Value) -> Result<()> {
        self.wal
            .lock()
            .unwrap()
            .write(&format!("SET {} {:?}", key, value))?;
        self.cache.lock().unwrap().put(key, value);
        Ok(())
    }
    fn get(&mut self, key: String) -> Option<Value> {
        self.cache.lock().unwrap().get(&key).cloned()
    }
}

impl WriteAheadLog {
    pub fn new(path: &str) -> Result<Self> {
        let file = OpenOptions::new().append(true).create(true).open(path)?;
        Ok(Self { file })
    }
    pub fn write(&mut self, entry: &str) -> Result<()> {
        writeln!(self.file, "{}", entry)?;
        self.file.flush()
    }
}

impl Snapshotter {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
        }
    }
    pub fn save_snapshot(&self, data: &str) -> Result<()> {
        let mut file = File::create(&self.path)?;
        file.write_all(data.as_bytes())?;
        Ok(())
    }
    pub fn load_snapshot(&self) -> Result<String> {
        let mut file = File::open(&self.path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(contents)
    }
}
