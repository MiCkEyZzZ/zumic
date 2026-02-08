pub trait HllHasher: Default + Clone {
    /// Хеширует срез байт в 64-битное значение.
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64;

    /// Возвращает имя хешера.
    fn name(&self) -> &'static str;
}

#[derive(Debug, Clone, Copy)]
pub struct MurmurHasher {
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

impl Default for MurmurHasher {
    /// Возвращает «значение по умолчанию» для типа. Значения по умолчанию часто
    /// представляют собой какое-либо начальное значение, значение идентичности
    /// или что-либо еще, что может иметь смысл в качестве значения по
    /// умолчанию.
    fn default() -> Self {
        Self::new(0)
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

/// MurmurHash3 64 битная реализация.
/// ВАЖНО: это упрощённая реализация и в дальнеёшем будет переход на `murmur3`.
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

    // Обработка 8-байтовых блоков
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

    // Обработка оставшихся байтов
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

    // Завершение
    h ^= len as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;

    h
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

        // Проверяем, что хеши хорошо распределены
        hashes.sort_unstable();
        hashes.dedup();

        // Должно быть почти 10000 уникальных хешей
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
