pub trait HllHasher: Default + Clone {
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64;

    fn name(&self) -> &'static str;
}

#[derive(Debug, Clone, Copy)]
pub struct MurmurHasher {
    seed: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct XxHasher {
    seed: u64,
}

impl MurmurHasher {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn with_default_seed() -> Self {
        Self::new(0)
    }
}

impl XxHasher {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn with_default_seed() -> Self {
        Self::new(0)
    }
}

impl Default for MurmurHasher {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Default for XxHasher {
    fn default() -> Self {
        Self::with_default_seed()
    }
}

impl HllHasher for MurmurHasher {
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64 {
        murmur3_64(bytes, self.seed)
    }

    fn name(&self) -> &'static str {
        "MurmurHash3"
    }
}

impl HllHasher for XxHasher {
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64 {
        xxhash64(bytes, self.seed)
    }

    fn name(&self) -> &'static str {
        "xxHash64"
    }
}

/// MurmurHash3 64-bit implementation.
/// NOTE: This is a simplified implementation and will be upgraded to `murmur3`
/// in the future.
fn murmur3_64(
    data: &[u8],
    seed: u64,
) -> u64 {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;
    const R1: u32 = 31;
    const R2: u32 = 27;
    const M: u64 = 5;

    let mut h = seed;
    let len = data.len();

    // Processing 8-byte chunks
    let chunks = data.chunks_exact(8);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let mut k = u64::from_le_bytes(chunk.try_into().unwrap());

        k = k.wrapping_mul(C1);
        k = k.rotate_left(R1);
        k = k.wrapping_mul(C2);

        h ^= k;
        h = h.rotate_left(R2);
        h = h.wrapping_mul(M).wrapping_add(0x52dce729);
    }

    // Processing the remaining bytes
    if !remainder.is_empty() {
        let mut k: u64 = 0;
        for (i, &byte) in remainder.iter().enumerate() {
            k |= (byte as u64) << (i * 8);
        }

        k = k.wrapping_mul(C1);
        k = k.rotate_left(R1);
        k = k.wrapping_mul(C2);
        h ^= k;
    }

    // Finalization
    h ^= len as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;

    h
}

/// xxhash64 implementation.
/// NOTE: This is a simplified implementation and will be upgraded to
/// `xxhash-rust` in the future.
fn xxhash64(
    data: &[u8],
    seed: u64,
) -> u64 {
    const PRIME1: u64 = 0x9e3779b185ebca87;
    const PRIME2: u64 = 0xc2b2ae3d27d4eb4f;
    const PRIME3: u64 = 0x165667b19e3779f9;
    const _PRIME4: u64 = 0x85ebca77c2b2ae63;
    const PRIME5: u64 = 0x27d4eb2f165667c5;

    let mut h64: u64;
    let len = data.len();

    if len >= 32 {
        let mut v1 = seed.wrapping_add(PRIME1).wrapping_add(PRIME2);
        let mut v2 = seed.wrapping_add(PRIME2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME1);

        let chunks = data.chunks_exact(32);
        for chunk in chunks {
            v1 = round(v1, u64::from_le_bytes(chunk[0..8].try_into().unwrap()));
            v2 = round(v2, u64::from_le_bytes(chunk[8..16].try_into().unwrap()));
            v3 = round(v3, u64::from_le_bytes(chunk[16..24].try_into().unwrap()));
            v4 = round(v4, u64::from_le_bytes(chunk[24..32].try_into().unwrap()));
        }

        h64 = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));

        h64 = merge_round(h64, v1);
        h64 = merge_round(h64, v2);
        h64 = merge_round(h64, v3);
        h64 = merge_round(h64, v4);
    } else {
        h64 = seed.wrapping_add(PRIME5);
    }

    h64 = h64.wrapping_add(len as u64);

    // Process remaining data (simplified)
    let remaining = &data[data.len() - (data.len() % 32)..];
    for &byte in remaining {
        h64 ^= (byte as u64).wrapping_mul(PRIME5);
        h64 = h64.rotate_left(11).wrapping_mul(PRIME1);
    }

    // Finalization
    h64 ^= h64 >> 33;
    h64 = h64.wrapping_mul(PRIME2);
    h64 ^= h64 >> 29;
    h64 = h64.wrapping_mul(PRIME3);
    h64 ^= h64 >> 32;

    h64
}

fn round(
    acc: u64,
    input: u64,
) -> u64 {
    const PRIME1: u64 = 0x9e3779b185ebca87;
    const PRIME2: u64 = 0xc2b2ae3d27d4eb4f;

    acc.wrapping_add(input.wrapping_mul(PRIME2))
        .rotate_left(31)
        .wrapping_mul(PRIME1)
}

fn merge_round(
    acc: u64,
    val: u64,
) -> u64 {
    const PRIME1: u64 = 0x9e3779b185ebca87;
    const PRIME2: u64 = 0xc2b2ae3d27d4eb4f;

    let val = round(0, val);
    acc ^ val.wrapping_mul(PRIME1).wrapping_add(PRIME2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_murmur_hasher() {
        let hasher = MurmurHasher::default();

        let hash1 = hasher.hash_bytes(b"foo");
        let hash2 = hasher.hash_bytes(b"foo");
        let hash3 = hasher.hash_bytes(b"bar");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hasher.name(), "MurmurHash3");
    }

    #[test]
    fn test_xxhash() {
        let hasher = XxHasher::default();

        let hash1 = hasher.hash_bytes(b"foo");
        let hash2 = hasher.hash_bytes(b"foo");
        let hash3 = hasher.hash_bytes(b"bar");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hasher.name(), "xxHash64");
    }

    #[test]
    fn test_murmur_seed() {
        let hasher1 = MurmurHasher::new(0);
        let hasher2 = MurmurHasher::new(42);

        let hash1 = hasher1.hash_bytes(b"foo");
        let hash2 = hasher2.hash_bytes(b"foo");

        assert_ne!(
            hash1, hash2,
            "Different seeds should produce different hashes"
        );
    }

    #[test]
    fn test_hash_distribution() {
        let hasher = MurmurHasher::default();
        let mut hashes = Vec::new();

        for i in 0..10000 {
            let data = format!("item_{i}");
            let hash = hasher.hash_bytes(data.as_bytes());
            hashes.push(hash);
        }

        // Check that the hashes are well distributed
        hashes.sort_unstable();
        hashes.dedup();

        // The should be almost 10_000 unique hashes
        assert!(
            hashes.len() > 9990,
            "Too many collisions: {}",
            10000 - hashes.len()
        );
    }

    #[test]
    fn test_single_byte() {
        let hasher = MurmurHasher::default();

        let hash1 = hasher.hash_bytes(&[0]);
        let hash2 = hasher.hash_bytes(&[1]);
        let hash3 = hasher.hash_bytes(&[255]);

        assert_ne!(hash1, hash2);
        assert_ne!(hash2, hash3);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_empty_input_determinism() {
        let hasher = MurmurHasher::default();

        let h1 = hasher.hash_bytes(b"");
        let h2 = hasher.hash_bytes(b"");

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_determinism_various_lengths() {
        let hasher = MurmurHasher::default();

        for len in 0..100 {
            let data = vec![42u8; len];
            assert_eq!(hasher.hash_bytes(&data), hasher.hash_bytes(&data));
        }
    }

    #[test]
    fn test_no_panic_on_random_data() {
        let hasher = MurmurHasher::default();

        for i in 0..1000 {
            let data = format!("random_{}_{}", i, i * i);
            let _ = hasher.hash_bytes(data.as_bytes());
        }
    }
}
