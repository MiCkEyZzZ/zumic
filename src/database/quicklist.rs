use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QuickList<T> {
    pub segments: Vec<VecDeque<T>>,
    pub max_segment_size: usize,
    pub len: usize,
    #[serde(skip)]
    pub index: HashMap<usize, usize>,
}

impl<T> QuickList<T> {
    pub fn new(max_segment_size: usize) -> Self {
        Self {
            segments: Vec::new(),
            max_segment_size,
            len: 0,
            index: HashMap::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        if let Some(seg_idx) = self.index.get(&index) {
            return self.segments[*seg_idx].get(*seg_idx);
        }
        let mut offset = 0;
        self.segments.iter().find_map(|segment| {
            if index < offset + segment.len() {
                segment.get(index - offset)
            } else {
                offset += segment.len();
                None
            }
        })
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }

        let mut remaining = index;
        for segment in &mut self.segments {
            if remaining < segment.len() {
                return segment.get_mut(remaining);
            } else {
                remaining -= segment.len();
            }
        }
        None
    }
    pub fn push_front(&mut self, item: T) {
        if self.segments.is_empty() || self.segments[0].len() >= self.max_segment_size {
            self.segments
                .insert(0, VecDeque::with_capacity(self.max_segment_size));
        }
        self.segments[0].push_front(item);
        self.len += 1;
        self.auto_optimize();
        self.rebuild_index();
    }
    pub fn push_back(&mut self, item: T) {
        if self.segments.is_empty() || self.segments.last().unwrap().len() >= self.max_segment_size
        {
            self.segments
                .push(VecDeque::with_capacity(self.max_segment_size));
        }
        self.segments.last_mut().unwrap().push_back(item);
        self.len += 1;
        self.auto_optimize();
        self.rebuild_index();
    }
    pub fn pop_front(&mut self) -> Option<T> {
        if self.segments.is_empty() {
            return None;
        }
        let item = self.segments[0].pop_front();
        if item.is_some() {
            self.len -= 1;
            if self.segments[0].is_empty() {
                self.segments.remove(0);
            }
            self.auto_optimize();
            self.rebuild_index();
        }
        item
    }
    pub fn pop_back(&mut self) -> Option<T> {
        if self.segments.is_empty() {
            return None;
        }

        let item = self.segments.last_mut().unwrap().pop_back();
        if item.is_some() {
            self.len -= 1;
            if self.segments.last().unwrap().is_empty() {
                self.segments.pop();
            }
            self.auto_optimize();
            self.rebuild_index();
        }
        item
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.segments.iter().flat_map(|seg| seg.iter())
    }
    pub fn clear(&mut self) {
        self.segments.clear();
        self.index.clear();
        self.len = 0;
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let mut total_len = 0;

        for segment in &self.segments {
            if segment.capacity() > self.max_segment_size * 2 {
                return Err("Segment capacity exceeds limit");
            }
            total_len += segment.len();
        }

        if total_len != self.len() {
            return Err("Length mismatch");
        }
        Ok(())
    }
    pub fn optimize(&mut self) {
        let mut new_segments = Vec::new();
        let mut current_segment = VecDeque::with_capacity(self.max_segment_size);

        for segment in self.segments.drain(..) {
            for item in segment {
                if current_segment.len() >= self.max_segment_size {
                    new_segments.push(current_segment);
                    current_segment = VecDeque::with_capacity(self.max_segment_size);
                }
                current_segment.push_back(item);
            }
        }

        if !current_segment.is_empty() {
            new_segments.push(current_segment);
        }

        self.segments = new_segments;
    }
    pub fn from_vecdeque(items: VecDeque<T>, max_segment_size: usize) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in items {
            qlist.push_back(item);
        }
        qlist
    }
    pub fn into_vecdeque(self) -> VecDeque<T> {
        let mut result = VecDeque::with_capacity(self.len);
        for mut segment in self.segments {
            result.append(&mut segment);
        }
        result
    }
    pub fn from_iter<I: IntoIterator<Item = T>>(iter: I, max_segment_size: usize) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in iter {
            qlist.push_back(item);
        }
        qlist
    }
    pub fn auto_optimize(&mut self) {
        if self.segments.len() > 5
            || self
                .segments
                .iter()
                .any(|s| s.len() < self.max_segment_size / 2)
        {
            self.optimize();
        }
    }
    pub fn shrink_to_fit(&mut self) {
        for segment in &mut self.segments {
            segment.shrink_to_fit();
        }
    }
    pub fn memory_usage(&self) -> usize {
        self.segments
            .iter()
            .map(|s| s.capacity() * std::mem::size_of::<T>())
            .sum()
    }
    pub fn rebuild_index(&mut self) {
        self.index.clear();
        let mut global_index = 0;
        for (seg_idx, segment) in self.segments.iter().enumerate() {
            for _ in segment {
                self.index.insert(global_index, seg_idx);
                global_index += 1;
            }
        }
    }
}
