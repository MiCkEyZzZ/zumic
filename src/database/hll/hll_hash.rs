use std::{hash::Hasher, io::Cursor, time::Instant};

use murmur3::murmur3_x64_128;
use siphasher::sip::SipHasher13;
use xxhash_rust::xxh64::xxh64;

use super::HllDense;

pub trait HllHasher: Default + Clone {
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64;

    fn name(&self) -> &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MurmurHasher {
    seed: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct XxHasher {
    seed: u64,
}

#[derive(Debug, Clone)]
pub struct SipHasher {
    key0: u64,
    key1: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct HashMetrics {
    /// Среднее время хещирования
    pub avg_hash_time_ns: f64,
    /// задержка p50
    pub p50_ns: f64,
    /// задержка p95
    pub p95_ns: f64,
    /// Пропускная способность (элементов в секунду)
    pub throughput_ops_sec: f64,
    /// Стандартное отклонение распредления индексов регистров
    pub register_index_stddev: f64,
    /// Среднее относительная ошибка HLL
    pub avg_relative_error: f64,
    /// Максимальная относительная ошибка HLL
    pub max_relative_error: f64,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

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

impl SipHasher {
    pub fn new(
        key0: u64,
        key1: u64,
    ) -> Self {
        Self { key0, key1 }
    }

    pub fn with_default_key() -> Self {
        Self::new(0, 0)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для MurmurHasher, XxHasher, SipHasher
////////////////////////////////////////////////////////////////////////////////

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

impl Default for SipHasher {
    fn default() -> Self {
        Self::with_default_key()
    }
}

impl HllHasher for MurmurHasher {
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64 {
        let mut cursor = Cursor::new(bytes);
        let hash128 = murmur3_x64_128(&mut cursor, self.seed as u32).unwrap();
        (hash128 >> 64) as u64
    }

    fn name(&self) -> &'static str {
        "MurmurHash"
    }
}

impl HllHasher for XxHasher {
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64 {
        xxh64(bytes, self.seed)
    }

    fn name(&self) -> &'static str {
        "XxHasher"
    }
}

impl HllHasher for SipHasher {
    fn hash_bytes(
        &self,
        bytes: &[u8],
    ) -> u64 {
        let mut hasher = SipHasher13::new_with_keys(self.key0, self.key1);
        hasher.write(bytes);
        hasher.finish()
    }

    fn name(&self) -> &'static str {
        "SipHash"
    }
}

pub fn benchmark_hasher<H: HllHasher, const P: usize>(
    hasher: &H,
    num_samples: usize,
) -> HashMetrics {
    let mut times = Vec::with_capacity(num_samples);
    let mut hll = HllDense::<P>::new();

    let m = 1 << P;
    let mut index_counts = vec![0usize; m];
    let mut errors = Vec::with_capacity(num_samples);
    let mut true_cardinality = 0u64;

    for i in 0..num_samples {
        let data = format!("item_{i}");
        let bytes = data.as_bytes();

        let start = Instant::now();
        let hash = hasher.hash_bytes(bytes);
        times.push(start.elapsed().as_nanos() as f64);

        // HLL
        let idx = (hash >> (64 - P)) as usize;
        let rho = ((hash << P).leading_zeros() + 1).min(64 - P as u32 + 1) as u8;

        if rho > hll.get_register(idx) {
            hll.set_register(idx, rho);
        }

        index_counts[idx] += 1;

        // относительная ошибка
        true_cardinality += 1;
        let est = hll.estimate();
        let rel_error = ((est as f64 - true_cardinality as f64) / true_cardinality as f64).abs();
        errors.push(rel_error);
    }

    // Метрики времени
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg_time = times.iter().sum::<f64>() / num_samples as f64;
    let p50 = times[num_samples / 2];
    let p95_idx = ((num_samples as f64 * 0.95).ceil() as usize).min(num_samples - 1);
    let p95 = times[p95_idx];
    let throughput_ops_sec = 1_000_000_000.0 / avg_time;

    // Стандартное отклонение распределения индексов регистров
    let mean = num_samples as f64 / m as f64;
    let var = index_counts
        .iter()
        .map(|&c| {
            let diff = c as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / m as f64;
    let register_index_stddev = var.sqrt();

    let avg_relative_error = errors.iter().sum::<f64>() / errors.len() as f64;
    let max_relative_error = errors.iter().cloned().reduce(f64::max).unwrap_or(0.0);

    HashMetrics {
        avg_hash_time_ns: avg_time,
        p50_ns: p50,
        p95_ns: p95,
        throughput_ops_sec,
        register_index_stddev,
        avg_relative_error,
        max_relative_error,
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hll_estimate_basic() {
        const P: usize = 10;
        let mut hll = HllDense::<P>::new();

        // В начало пустой HLL
        assert_eq!(hll.estimate(), 0);

        // Устанавляиваем несколько регистров вручную
        hll.set_register(0, 1);
        hll.set_register(1, 2);
        hll.set_register(2, 3);

        let est = hll.estimate();

        // Должно быть ненулевым, хотя точность маленькая
        assert!(est >= 3, "estimate should be >= 3, got {est}");
    }

    #[test]
    fn test_benchmark_hasher_runs() {
        const P: usize = 10;
        let hasher = MurmurHasher::default();
        let metrics = benchmark_hasher::<_, P>(&hasher, 1000);

        // Проверяем базовые метрики на разумные значения
        assert!(metrics.avg_hash_time_ns > 0.0);
        assert!(metrics.p50_ns > 0.0);
        assert!(metrics.p95_ns > 0.0);
        assert!(metrics.throughput_ops_sec > 0.0);
        assert!(metrics.register_index_stddev >= 0.0);
        assert!(metrics.avg_relative_error >= 0.0);
        assert!(metrics.max_relative_error >= 0.0);
    }

    #[test]
    fn test_hll_relative_error_small() {
        const P: usize = 12;
        let hasher = XxHasher::default();
        let metrics = benchmark_hasher::<_, P>(&hasher, 5000);

        // Для разумного числа элементов HLL ошибка < 10%
        assert!(metrics.avg_relative_error < 0.1, "avg error too high");
        assert!(metrics.max_relative_error < 0.25, "max error too high");
    }

    #[test]
    fn test_hll_register_stddev() {
        const P: usize = 10;
        let hasher = SipHasher::default();
        let metrics = benchmark_hasher::<_, P>(&hasher, 10000);

        // Стандартное отклонение должно быть меньше ~2 * среднее
        let expected_mean = 10000.0 / (1 << P) as f64;
        assert!(metrics.register_index_stddev < expected_mean * 2.0);
    }

    #[test]
    fn test_murmur_hasher() {
        let hasher = MurmurHasher::default();

        let hash1 = hasher.hash_bytes(b"foo");
        let hash2 = hasher.hash_bytes(b"foo");
        let hash3 = hasher.hash_bytes(b"bar");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hasher.name(), "MurmurHash");
    }

    #[test]
    fn test_murmur_high_bits_vary() {
        let h = MurmurHasher::default();

        // две строки, отличающиеся в «высоких» частях
        let a = b"\x80\x00\x00\x00low";
        let b = b"\x00\x00\x00\x00low";

        let ha = h.hash_bytes(a);
        let hb = h.hash_bytes(b);

        assert_ne!(
            ha, hb,
            "high 64 bits should differ for inputs with different high bytes"
        );
    }

    #[test]
    fn test_xxhash() {
        let hasher = XxHasher::default();

        let hash1 = hasher.hash_bytes(b"foo");
        let hash2 = hasher.hash_bytes(b"foo");
        let hash3 = hasher.hash_bytes(b"bar");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hasher.name(), "XxHasher");
    }

    #[test]
    fn test_siphash() {
        let hasher = SipHasher::default();

        let hash1 = hasher.hash_bytes(b"foo");
        let hash2 = hasher.hash_bytes(b"foo");
        let hash3 = hasher.hash_bytes(b"bar");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hasher.name(), "SipHash");
    }

    #[test]
    fn test_siphash_different_keys() {
        let h1 = SipHasher::new(0, 0);
        let h2 = SipHasher::new(1, 0);
        let h3 = SipHasher::new(0, 1);

        let data = b"hello world";

        let r1 = h1.hash_bytes(data);
        let r2 = h2.hash_bytes(data);
        let r3 = h3.hash_bytes(data);

        assert_ne!(r1, r2);
        assert_ne!(r1, r3);
        assert_ne!(r2, r3);
    }

    #[test]
    fn test_siphash_empty_input() {
        let hasher = SipHasher::default();

        let h1 = hasher.hash_bytes(b"");
        let h2 = hasher.hash_bytes(b"");

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_siphash_various_lengths() {
        let hasher = SipHasher::default();

        for len in 0..128 {
            let data = vec![42u8; len];
            let h1 = hasher.hash_bytes(&data);
            let h2 = hasher.hash_bytes(&data);
            assert_eq!(h1, h2);
        }
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

    #[test]
    fn test_hll_index_distribution() {
        const P: usize = 10;
        let hasher = XxHasher::default();
        let mut buckets = vec![0usize; 1 << P];

        for i in 0..100_000 {
            let h = hasher.hash_bytes(format!("k{i}").as_bytes());
            let idx = (h >> (64 - P)) as usize;
            buckets[idx] += 1;
        }

        let min = *buckets.iter().min().unwrap();
        let max = *buckets.iter().max().unwrap();

        assert!(max - min < 1000);
    }
}
