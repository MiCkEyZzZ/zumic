use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

const NUM_REGISTERS: usize = 16_384;
const REGISTER_BITS: usize = 6;
pub const DENSE_SIZE: usize = NUM_REGISTERS * REGISTER_BITS / 8; // 12288 байт

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HLL {
    #[serde(with = "BigArray")]
    pub data: [u8; DENSE_SIZE],
}

impl HLL {
    pub fn new() -> Self {
        Self {
            data: [0; DENSE_SIZE],
        }
    }

    /// Add an element to HeperLogLog.
    pub fn add(&mut self, value: &[u8]) {
        let hash = Self::hash(value);
        let (index, rho) = Self::index_and_rho(hash);
        let current = self.get_register(index);
        if rho > current {
            self.set_register(index, rho);
        }
    }

    /// Estimate the cardinality of a set.
    pub fn estimate_cardinality(&self) -> f64 {
        let mut sum = 0.0;
        let mut zeros = 0;

        for i in 0..NUM_REGISTERS {
            let val = self.get_register(i);
            if val == 0 {
                zeros += 1;
            }
            sum += 1.0 / (1_u64 << val) as f64;
        }

        let m = NUM_REGISTERS as f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let raw_estimate = alpha * m * m / sum;

        if raw_estimate <= 2.5 * m && zeros != 0 {
            m * (m / zeros as f64).ln()
        } else {
            raw_estimate
        }
    }

    fn hash(value: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn index_and_rho(hash: u64) -> (usize, u8) {
        let index = (hash >> (64 - 14)) as usize;
        let remaining = hash << 14 | 1 << 13;
        let rho = remaining.leading_zeros() as u8 + 1;
        (index, rho)
    }

    fn get_register(&self, index: usize) -> u8 {
        let bit_index = index * 6;
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;

        let byte1 = self.data[byte_index];
        let byte2 = if byte_index + 1 < DENSE_SIZE {
            self.data[byte_index + 1]
        } else {
            0
        };

        let combined = ((byte2 as u16) << 8) | byte1 as u16;
        ((combined >> bit_offset) & 0x3F) as u8
    }

    fn set_register(&mut self, index: usize, value: u8) {
        let bit_index = index * 6;
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;

        let mut combined = (self.data[byte_index] as u16)
            | ((self.data.get(byte_index + 1).cloned().unwrap_or(0) as u16) << 8);
        combined &= !(0x3F << bit_offset);
        combined |= (value as u16 & 0x3F) << bit_offset;
        self.data[byte_index] = (combined & 0xFF) as u8;
        if byte_index + 1 < DENSE_SIZE {
            self.data[byte_index + 1] = (combined >> 8) as u8;
        }
    }
}
