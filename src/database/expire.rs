use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    time::{Duration, Instant},
};

pub struct ExpireMap {
    deadlines: HashMap<Vec<u8>, Instant>,
    queue: BinaryHeap<Reverse<(Instant, Vec<u8>)>>,
}

impl ExpireMap {
    pub fn new() -> Self {
        Self {
            deadlines: HashMap::new(),
            queue: BinaryHeap::new(),
        }
    }

    pub fn set(
        &mut self,
        key: Vec<u8>,
        ttl: Duration,
    ) {
        let deadline = Instant::now() + ttl;
        self.deadlines.insert(key.clone(), deadline);
        self.queue.push(Reverse((deadline, key)));
    }

    pub fn get(
        &mut self,
        key: &[u8],
    ) -> bool {
        self.purge();
        self.deadlines.contains_key(key)
    }

    pub fn remove(
        &mut self,
        key: &[u8],
    ) {
        self.deadlines.remove(key);
        // BinaryHeap не поддерживает удаление по ключу, но это не критично:
        // просроченные ключи будут проигнорированы при purge.
    }

    pub fn purge(&mut self) -> Vec<Vec<u8>> {
        let now = Instant::now();
        let mut expired = Vec::new();
        while let Some(Reverse((deadline, ref key))) = self.queue.peek() {
            if *deadline > now {
                break;
            }
            let key = key.clone();
            self.queue.pop();
            if let Some(sorted_deadline) = self.deadlines.get(&key) {
                if *sorted_deadline <= now {
                    self.deadlines.remove(&key);
                    expired.push(key);
                }
            }
        }
        expired
    }
}

impl Default for ExpireMap {
    fn default() -> Self {
        Self::new()
    }
}
