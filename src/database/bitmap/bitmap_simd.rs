use crate::database::BIT_COUNT_TABLE;

/// Стратегия вычисления кол-ва установленных битов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcountStrategy {
    /// Таблица поиска (базовый вариант, всегда доступен)
    LookupTable,
    /// Инструкция POPCNT (x86_64 с SSE4.2)
    Popcnt,
    /// AVX2 SIMD (256-битные векторы)
    Avx2,
    /// AVX-512 SIMD (512-битные векторы)
    Avx512,
}

/// Результат обнаружения возможностей процессора.
#[derive(Debug, Clone, Copy)]
pub struct CpuFeatures {
    /// Наличие инструкции POPCNT
    pub has_popcnt: bool,
    /// Наличие AVX2
    pub has_avx2: bool,
    /// Наличие AVX-512
    pub has_avx512: bool,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl CpuFeatures {
    /// Определяет доступные возможности процессора во время выполнения.
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self {
                has_popcnt: is_x86_feature_detected!("popcnt"),
                has_avx2: is_x86_feature_detected!("avx2"),
                // FIX 2: AVX-512 popcnt требует avx512vpopcntdq
                has_avx512: is_x86_feature_detected!("avx512f")
                    && is_x86_feature_detected!("avx512bw")
                    && is_x86_feature_detected!("avx512vpopcntdq"),
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        {
            Self {
                has_popcnt: false,
                has_avx2: false,
                has_avx512: false,
            }
        }
    }

    /// Возвращает наиболее подходящую доступную стратегию подсчёта битов.
    pub fn best_strategy(&self) -> BitcountStrategy {
        if self.has_avx512 {
            BitcountStrategy::Avx512
        } else if self.has_avx2 {
            BitcountStrategy::Avx2
        } else if self.has_popcnt {
            BitcountStrategy::Popcnt
        } else {
            BitcountStrategy::LookupTable
        }
    }
}

/// Базовый подсчёт битов с использованием таблицы поиска.
#[inline]
pub fn bitcount_lookup_table(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .map(|&b| BIT_COUNT_TABLE[b as usize] as usize)
        .sum()
}

/// Безопасная оболочка для подсчёта битов с использованием POPCNT.
#[inline]
pub fn bitcount_popcnt(bytes: &[u8]) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("popcnt") {
            unsafe { bitcount_popcnt_impl(bytes) }
        } else {
            bitcount_lookup_table(bytes)
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        bitcount_lookup_table(bytes)
    }
}

/// Безопасная обёртка для подсчёта битов с использованием AVX2.
pub fn bitcount_avx2(bytes: &[u8]) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { bitcount_avx2_impl(bytes) }
        } else {
            bitcount_popcnt(bytes)
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        bitcount_lookup_table(bytes)
    }
}

/// Безопасный обёртка для подсчёта битов с использованием AVX-512
#[inline]
pub fn bitcount_avx512(bytes: &[u8]) -> usize {
    #[cfg(all(feature = "avx512", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512f")
            && is_x86_feature_detected!("avx512bw")
            && is_x86_feature_detected!("avx512vpopcntdq")
        {
            unsafe { bitcount_avx512_impl(bytes) }
        } else {
            bitcount_avx2(bytes)
        }
    }

    #[cfg(not(all(feature = "avx512", target_arch = "x86_64")))]
    {
        bitcount_lookup_table(bytes)
    }
}

/// Автоматический выбор и выполнение оптимальной стратегии подсчёта битов.
#[inline]
pub fn bitcount_auto(bytes: &[u8]) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512f")
            && is_x86_feature_detected!("avx512bw")
            && is_x86_feature_detected!("avx512vpopcntdq")
        {
            bitcount_avx512(bytes)
        } else if is_x86_feature_detected!("avx2") {
            bitcount_avx2(bytes)
        } else if is_x86_feature_detected!("popcnt") {
            bitcount_popcnt(bytes)
        } else {
            bitcount_lookup_table(bytes)
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        bitcount_lookup_table(bytes)
    }
}

/// Подсчёт битов с явным выбором стратегии.
pub fn bitcount_with_strategy(
    bytes: &[u8],
    strategy: BitcountStrategy,
) -> usize {
    match strategy {
        BitcountStrategy::LookupTable => bitcount_lookup_table(bytes),
        BitcountStrategy::Popcnt => bitcount_popcnt(bytes),
        BitcountStrategy::Avx2 => bitcount_avx2(bytes),
        BitcountStrategy::Avx512 => bitcount_avx512(bytes),
    }
}

////////////////////////////////////////////////////////////////////////////////
// Внутренние методы и функции
////////////////////////////////////////////////////////////////////////////////

/// Подсчёт битов с использованием инструкции POPCNT (x86_64).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "popcnt")]
unsafe fn bitcount_popcnt_impl(bytes: &[u8]) -> usize {
    let mut count = 0usize;
    let mut i = 0usize;

    while i + 8 <= bytes.len() {
        let v = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        count += v.count_ones() as usize;
        i += 8;
    }

    while i < bytes.len() {
        count += BIT_COUNT_TABLE[bytes[i] as usize] as usize;
        i += 1;
    }

    count
}

/// Подсчёт битов с использованием AVX2 SIMD (256-битные векторы)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn bitcount_avx2_impl(bytes: &[u8]) -> usize {
    use std::arch::x86_64::*;

    let mut count = 0usize;
    let mut ptr = bytes.as_ptr();
    let end = ptr.add(bytes.len());

    let lookup = _mm256_setr_epi8(
        0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4, 0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3,
        3, 4,
    );
    let low_mask = _mm256_set1_epi8(0x0f);

    while ptr.add(32) <= end {
        let vec = _mm256_loadu_si256(ptr as *const __m256i);

        let lo = _mm256_and_si256(vec, low_mask);
        let hi = _mm256_and_si256(_mm256_srli_epi16(vec, 4), low_mask);

        let sum = _mm256_add_epi8(
            _mm256_shuffle_epi8(lookup, lo),
            _mm256_shuffle_epi8(lookup, hi),
        );

        let sad = _mm256_sad_epu8(sum, _mm256_setzero_si256());
        let mut tmp = [0u64; 4];
        _mm256_storeu_si256(tmp.as_mut_ptr() as *mut __m256i, sad);
        count += tmp.iter().sum::<u64>() as usize;

        ptr = ptr.add(32);
    }

    while ptr.add(8) <= end {
        count += _popcnt64((ptr as *const u64).read_unaligned() as i64) as usize;
        ptr = ptr.add(8);
    }

    while ptr < end {
        count += BIT_COUNT_TABLE[*ptr as usize] as usize;
        ptr = ptr.add(1);
    }

    count
}

/// Подсчёт битов с использованием AVX-512 SIMD (512-битные векторы)
/// Компилируется ТОЛЬКО если включена фича "avx512" и таргет x86_64.
/// Это гарантирует, что на stable без фичи AVX-512 не будет ошибок E0658
#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f,avx512bw,avx512vpopcntdq")]
// FIX 3: добавлен avx512vpopcntdq + убран reduce_add
unsafe fn bitcount_avx512_impl(bytes: &[u8]) -> usize {
    use std::arch::x86_64::*;

    let mut count = 0usize;
    let mut ptr = bytes.as_ptr();
    let end = ptr.add(bytes.len());

    while ptr.add(64) <= end {
        let vec = _mm512_loadu_si512(ptr as *const __m512i);
        let pop = _mm512_popcnt_epi64(vec);

        let mut tmp = [0u64; 8];
        _mm512_storeu_si512(tmp.as_mut_ptr() as *mut __m512i, pop);
        count += tmp.iter().sum::<u64>() as usize;

        ptr = ptr.add(64);
    }

    while ptr.add(8) <= end {
        count += _popcnt64((ptr as *const u64).read_unaligned() as i64) as usize;
        ptr = ptr.add(8);
    }

    while ptr < end {
        count += BIT_COUNT_TABLE[*ptr as usize] as usize;
        ptr = ptr.add(1);
    }

    count
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_features() {
        let features = CpuFeatures::detect();
        println!("CPU Features: {features:?}");
        println!("Best strategy: {:?}", features.best_strategy());

        // All platforms should at least have lookup table.
        assert!(matches!(
            features.best_strategy(),
            BitcountStrategy::LookupTable
                | BitcountStrategy::Popcnt
                | BitcountStrategy::Avx2
                | BitcountStrategy::Avx512
        ));
    }
}
