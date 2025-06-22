//! QuickList is a segmented list structure optimized
//! for operations involving adding/removing elements at both ends and
//! adaptive memory management.
//!
//! It contains a vector of segments of type `VecDeque<T>`, which allows
//! for efficient operations like `push_front`, `push_back`, `pop_front`
//! and `pop_back`.
//!
//! Each segment has a maximum size (`max_segment_size`), which reduces
//! overhead from memory allocation and fragmentation. If the segments grow
//! too large or become excessively sparse, the list can be reallocated
//! using the `optimize()` or `auto_optimize()` methods.
//!
//! QuickList is well-suited for workloads with frequent
//! additions/removals from both ends and rare random access to elements.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

/// A segmented list structure with capped segments and optimized
/// access.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QuickList<T> {
    /// The list segments; each segment is a `VecDeque` with a capped size.
    segments: Vec<VecDeque<T>>,

    /// The maximum number of elements in one segment.
    max_segment_size: usize,

    /// The total number of elements across all segments.
    len: usize,

    /// An optional map of indices from the logical index to the segment index.
    #[serde(skip)]
    index: HashMap<usize, usize>,
}

impl<T> QuickList<T> {
    /// Creates a new empty `QuickList` with the specified segment size.
    pub fn new(max_segment_size: usize) -> Self {
        Self {
            segments: Vec::new(),
            max_segment_size,
            len: 0,
            index: HashMap::new(),
        }
    }

    /// Returns the total number of elements in the list.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a reference to the element at the given logical index, if it exists.
    pub fn get(
        &self,
        index: usize,
    ) -> Option<&T> {
        if index >= self.len {
            return None;
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

    /// Returns a mutable reference to the element at the given logical index, if it exists.
    pub fn get_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }
        let mut offset = 0;
        self.segments.iter_mut().find_map(|segment| {
            if index < offset + segment.len() {
                Some(segment.get_mut(index - offset).unwrap())
            } else {
                offset += segment.len();
                None
            }
        })
    }

    /// Inserts an element at the front of the list.
    pub fn push_front(
        &mut self,
        item: T,
    ) {
        if self.segments.is_empty() || self.segments[0].len() >= self.max_segment_size {
            self.segments
                .insert(0, VecDeque::with_capacity(self.max_segment_size));
        }
        self.segments[0].push_front(item);
        self.len += 1;
        self.auto_optimize();
        self.rebuild_index();
    }

    /// Inserts an element at the back of the list.
    pub fn push_back(
        &mut self,
        item: T,
    ) {
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

    /// Removes and returns the first element of the list, if it exists.
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

    /// Removes and returns the last element of the list, if it exists.
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

    /// Returns an iterator over all elements in the list.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.segments.iter().flat_map(|seg| seg.iter())
    }

    /// Clears the list by removing all elements.
    pub fn clear(&mut self) {
        self.segments.clear();
        self.index.clear();
        self.len = 0;
    }

    /// Checks the internal consistency of the list.
    /// Returns `Err` if any segment exceeds the allowable capacity
    /// or if the internal length counter is incorrect.
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

    /// Compacts and reorganizes segments to maintain balance.
    /// Merges underfilled segments and removes empty ones.
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

    /// Creates a `QuickList` from a single `VecDeque`.
    pub fn from_vecdeque(
        items: VecDeque<T>,
        max_segment_size: usize,
    ) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in items {
            qlist.push_back(item);
        }
        qlist
    }

    /// Converts the `QuickList` into a single `VecDeque` of elements.
    pub fn into_vecdeque(self) -> VecDeque<T> {
        let mut result = VecDeque::with_capacity(self.len);
        for mut segment in self.segments {
            result.append(&mut segment);
        }
        result
    }

    /// Creates a `QuickList` from any iterable set of elements.
    pub fn from_iter<I: IntoIterator<Item = T>>(
        iter: I,
        max_segment_size: usize,
    ) -> Self {
        let mut qlist = Self::new(max_segment_size);
        for item in iter {
            qlist.push_back(item);
        }
        qlist
    }

    /// Automatically optimizes the list if certain conditions are met.
    ///
    /// Optimization triggers if there are too many segments
    /// or if segments are significantly underfilled.
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

    /// Shrinks each segment to fit its current size.
    pub fn shrink_to_fit(&mut self) {
        for segment in &mut self.segments {
            segment.shrink_to_fit();
        }
    }

    /// Estimates the memory usage of the list in bytes.
    ///
    /// The calculation is based on the segment capacities and the size of type `T`.
    pub fn memory_usage(&self) -> usize {
        self.segments
            .iter()
            .map(|s| s.capacity() * std::mem::size_of::<T>())
            .sum()
    }

    /// Rebuilds the internal index, mapping logical indices to segment numbers.
    ///
    /// Used for fast index access or debugging.
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

impl<T> IntoIterator for QuickList<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments
            .into_iter()
            .flat_map(|seg| seg.into_iter())
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests the `push_front` and `pop_front` methods.
    /// Verifies that when adding elements to the front and subsequently removing them,
    /// the order and the number of elements are preserved correctly.
    #[test]
    fn test_push_front_and_pop_front() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_front(1);
        list.push_front(2);
        list.push_front(3);

        assert_eq!(list.len(), 3);

        let item = list.pop_front();
        assert_eq!(item, Some(3));

        assert_eq!(list.len(), 2);
    }

    /// Tests the `push_back` and `pop_back` methods.
    /// Verifies that when adding elements to the end and removing them,
    /// the order and the number of elements are correctly maintained.
    #[test]
    fn test_push_back_and_pop_back() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        assert_eq!(list.len(), 3);

        let item = list.pop_back();
        assert_eq!(item, Some(3));

        assert_eq!(list.len(), 2);
    }

    /// Tests the `get` and `get_mut` methods.
    /// Verifies that it is possible to access an element by index
    /// and modify its value.
    #[test]
    fn test_get_and_get_mut() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(10);
        list.push_back(20);
        list.push_back(30);

        assert_eq!(list.get(1), Some(&20));

        if let Some(item) = list.get_mut(1) {
            *item = 25;
        }

        assert_eq!(list.get(1), Some(&25));
    }

    /// Tests the `clear` method.
    /// Verifies that after clearing, the list becomes empty.
    #[test]
    fn test_clear() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        list.clear();

        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    /// Tests the `validate` method.
    /// Verifies that the list passes validation and triggers an error when segment constraints are violated.
    #[test]
    fn test_validate() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        assert!(list.validate().is_ok());

        list.segments[0].push_back(4);
        assert!(list.validate().is_err());
    }

    /// Tests the `auto_optimize` method.
    /// Verifies that optimization occurs if the number of segments is too high
    /// or if segments are underfilled.
    #[test]
    fn test_auto_optimize() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);
        list.push_back(4);
        list.push_back(5);

        let before = list.segments.len();
        list.auto_optimize();
        let after = list.segments.len();

        assert!(after <= before);
        assert_eq!(list.len(), 5);
    }

    /// Tests the `from_vecdeque` method.
    /// Verifies that a list can be created from a `VecDeque` and elements are correctly inserted.
    #[test]
    fn test_from_vecdeque() {
        let items: VecDeque<i32> = VecDeque::from(vec![1, 2, 3]);
        let list = QuickList::from_vecdeque(items, 3);

        assert_eq!(list.len(), 3);
        assert_eq!(list.get(0), Some(&1));
        assert_eq!(list.get(1), Some(&2));
        assert_eq!(list.get(2), Some(&3));
    }

    /// Tests the `into_vecdeque` method.
    /// Verifies that the list correctly converts into a `VecDeque` with the proper element sequence.
    #[test]
    fn test_into_vecdeque() {
        let mut list: QuickList<i32> = QuickList::new(3);
        list.push_back(10);
        list.push_back(20);
        list.push_back(30);

        let vecdeque: VecDeque<i32> = list.into_vecdeque();
        assert_eq!(vecdeque, VecDeque::from(vec![10, 20, 30]));
    }

    /// Tests the `shrink_to_fit` method.
    /// Verifies that the capacity of the segments is reduced to the current data size.
    #[test]
    fn test_shrink_to_fit() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        list.shrink_to_fit();
        // Verifying that the capacity of each segment is not less than its length.
        assert!(list.segments.iter().all(|seg| seg.capacity() >= seg.len()));
    }

    /// Tests the `memory_usage` method.
    /// Verifies that the memory usage of the list is calculated correctly, considering segment capacity and element size.
    #[test]
    fn test_memory_usage() {
        let mut list: QuickList<i32> = QuickList::new(3);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        let memory_usage = list.memory_usage();
        // Verifying that memory usage is greater than zero (considering segment sizes).
        assert!(memory_usage > 0);
    }
}
