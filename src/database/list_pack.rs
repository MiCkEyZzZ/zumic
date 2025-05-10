//! The `ListPack` module implements a compact structure for storing
//! variable-length elements, optimized for minimal memory usage and serialization.
//!
//! `ListPack` is a data structure similar to Redis ListPacks, storing
//! a sequence of byte elements. Each element is prefixed with its length
//! encoded as a variable-length integer (varint) and the entire list is
//! terminated with a special byte 0xFF.

pub struct ListPack {
    data:        Vec<u8>,
    head:        usize,
    tail:        usize,
    num_entries: usize,
}

impl ListPack {
    /// Creates a new empty `ListPack` with preallocated capacity and
    /// a terminator at the center.
    pub fn new() -> Self {
        let cap = 1024;
        let mut data = Vec::with_capacity(cap);
        data.resize(cap, 0);
        let head = cap / 2;
        data[head] = 0xFF;
        Self {
            data,
            head,
            tail: head + 1,
            num_entries: 0,
        }
    }

    /// Amortized expansion and centering of the internal buffer if needed.
    /// Ensures there is enough space to insert `extra` bytes.
    fn grow_and_center(&mut self, extra: usize) {
        let used = self.tail - self.head;
        let need = used + extra + 1;
        if need <= self.data.len() {
            // Already enough space.
            return;
        }
        // New capacity: max(1.5x, need)
        let new_cap = (self.len().max(1) * 3 / 2).max(need);
        let mut new_data = Vec::with_capacity(new_cap);
        new_data.resize(new_cap, 0);
        // Center the current content in the new buffer.
        let new_head = (new_cap - used) / 2;
        new_data[new_head..new_head + used].copy_from_slice(&self.data[self.head..self.tail]);
        self.head = new_head;
        self.tail = new_head + used;
        self.data = new_data;
    }

    /// Inserts a value at the front of the list.
    pub fn push_front(&mut self, value: &[u8]) {
        let mut len_bytes = Vec::new();
        let mut v = value.len();
        while v >= 0x80 {
            len_bytes.push((v as u8 & 0x7F) | 0x80);
            v >>= 7;
        }
        len_bytes.push(v as u8);

        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        // Move head backward and write len + value
        self.head -= extra;
        let h = self.head;
        // len
        self.data[h..h + len_bytes.len()].copy_from_slice(&len_bytes);
        // payload
        self.data[h + len_bytes.len()..h + extra].copy_from_slice(value);

        self.num_entries += 1;
    }

    /// Inserts a value at the back of the list.
    pub fn push_back(&mut self, value: &[u8]) {
        // Encode the length as varint
        let mut len_bytes = Vec::new();
        let mut v = value.len();
        while v >= 0x80 {
            len_bytes.push((v as u8 & 0x7F) | 0x80);
            v >>= 7;
        }
        len_bytes.push(v as u8);

        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        // Overwrite the current terminator (0xFF) at tail - 1,
        // then write len + value + new 0xFF
        let term_pos = self.tail - 1;
        // len
        self.data[term_pos..term_pos + len_bytes.len()].copy_from_slice(&len_bytes);
        // payload
        let vstart = term_pos + len_bytes.len();
        self.data[vstart..vstart + value.len()].copy_from_slice(value);
        // New terminator
        let new_term = vstart + value.len();
        self.data[new_term] = 0xFF;

        self.tail = new_term + 1;
        self.num_entries += 1;
    }

    /// Returns the number of entries in the list.
    pub fn len(&self) -> usize {
        self.num_entries
    }

    /// Returns a reference to the element at the specified index, if it exists.
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        if index >= self.num_entries {
            return None;
        }

        let mut i = self.head;
        let mut curr = 0;

        while i < self.tail {
            if self.data[i] == 0xFF {
                break;
            }

            let (len, consumed) = Self::decode_varint(&self.data[i..])?;
            if curr == index {
                return Some(&self.data[i + consumed..i + consumed + len]);
            }
            i += consumed + len;
            curr += 1;
        }

        None
    }

    /// Returns an iterator over all elements in the list.
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        let data = &self.data;
        let mut pos = self.head;
        let end = self.tail;

        std::iter::from_fn(move || {
            if pos >= end || data[pos] == 0xFF {
                return None;
            }

            let (len, consumed) = ListPack::decode_varint(&data[pos..])?;
            let start = pos + consumed;
            let slice = &data[start..start + len];
            pos = start + len;
            Some(slice)
        })
    }

    /// Removes the element at the specified index. Returns true if successful.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.num_entries {
            return false;
        }

        let mut i = self.head;
        let mut curr = 0;

        while i < self.tail {
            if self.data[i] == 0xFF {
                break;
            }

            if let Some((len, consumed)) = Self::decode_varint(&self.data[i..]) {
                if curr == index {
                    let start = i;
                    let end = i + consumed + len;

                    // Shift everything after `end` left to `start`
                    let _to_move = self.tail - end;
                    self.data.copy_within(end..self.tail, start);
                    self.tail -= end - start;

                    // Update the terminator
                    if self.tail > 0 {
                        self.data[self.tail - 1] = 0xFF;
                    }

                    self.num_entries -= 1;
                    return true;
                }

                i += consumed + len;
                curr += 1;
            } else {
                break;
            }
        }

        false
    }

    /// Encodes a `usize` value as a variable-length integer (varint).
    pub fn encode_variant(mut value: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8; // Take lowest 7 bits
            value >>= 7;
            if value == 0 {
                buf.push(byte); // Last byte: continuation bit is not set
                break;
            } else {
                buf.push(byte | 0x80); // Set continuation bit (more bytes follow)
            }
        }
        buf
    }

    /// Decodes a variable-length integer (varint) from the given slice.
    /// Returns the decoded value and the number of bytes consumed.
    pub fn decode_varint(data: &[u8]) -> Option<(usize, usize)> {
        let mut result = 0usize;
        let mut shift = 0;
        for (i, &byte) in data.iter().enumerate() {
            result |= ((byte & 0x7F) as usize) << shift;
            if byte & 0x80 == 0 {
                return Some((result, i + 1));
            }
            shift += 7;
            if shift > std::mem::size_of::<usize>() * 8 {
                return None;
            }
        }
        None
    }
}
