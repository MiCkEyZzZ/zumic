use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QuickList<T> {
    pub segments: Vec<VecDeque<T>>,
    pub max_segment_size: usize,
    pub len: usize,
    #[serde(skip)]
    pub index: HashMap<usize, usize>, // Кэш: глобальный индекс -> (сегмент, локальный индекс)
}

impl<T> QuickList<T> {
    pub fn rebuild_index(&mut self) {
        self.index.clear();
        let mut global = 0;
        for (seg_i, segment) in self.segments.iter().enumerate() {
            for _ in 0..segment.len() {
                self.index.insert(global, seg_i);
                global += 1;
            }
        }
    }
}
