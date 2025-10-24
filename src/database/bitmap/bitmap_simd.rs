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

impl CpuFeatures {
    /// Определяет доступные возможности процессора во время выполнения.
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self {
                has_popcnt: is_x86_feature_detected!("popcnt"),
                has_avx2: is_x86_feature_detected!("avx2"),
                has_avx512: is_x86_feature_detected!("avx512f")
                    && is_x86_feature_detected!("avx512bw"),
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
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("avx512bw") {
            unsafe { bitcount_avx512_impl(bytes) }
        } else {
            bitcount_avx2(bytes)
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        bitcount_lookup_table(bytes)
    }
}

/// Автоматический выбор и выполнение оптимальной стратегии подсчёта битов.
#[inline]
pub fn bitcount_auto(bytes: &[u8]) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("avx512bw") {
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

/// Подсчёт битов с использованием инструкции POPCNT (x86_64).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "popcnt")]
unsafe fn bitcount_popcnt_impl(bytes: &[u8]) -> usize {
    // Обрабатываем по 8 байт (u64) блокам, используя portable count_ones()
    let mut count: usize = 0;
    let mut i = 0usize;
    let len = bytes.len();

    // Проходим по u64 блокам
    while i + 8 <= len {
        let mut chunk_bytes = [0u8; 8];
        // safe copy — alignment не важен
        chunk_bytes.copy_from_slice(&bytes[i..i + 8]);
        let v = u64::from_le_bytes(chunk_bytes);
        count += v.count_ones() as usize;
        i += 8;
    }

    // Оставшиеся байты
    while i < len {
        count += BIT_COUNT_TABLE[bytes[i] as usize] as usize;
        i += 1;
    }

    count
}

/// Подсчёт битов с использованием AVX2 SIMD (256-битные векторы)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn bitcount_avx2_impl(bytes: &[u8]) -> usize {
    use std::arch::x86_64::{_mm256_set1_epi8, _mm256_setr_epi8};

    let mut count = 0usize;
    let mut ptr = bytes.as_ptr();
    let end = unsafe { ptr.add(bytes.len()) };

    // Таблица для подсчёта битов в 4-битных половинках байта (nibbles)
    let lookup = _mm256_setr_epi8(
        0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4, 0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3,
        3, 4,
    );
    let low_mask = _mm256_set1_epi8(0x0f);

    // Обрабатываем по 32 байта за раз
    while unsafe { ptr.add(32) <= end } {
        use std::arch::x86_64::{
            __m256i, _mm256_add_epi8, _mm256_and_si256, _mm256_extracti128_si256,
            _mm256_loadu_si256, _mm256_sad_epu8, _mm256_setzero_si256, _mm256_shuffle_epi8,
            _mm256_srli_epi16, _mm_extract_epi64,
        };

        let vec = unsafe { _mm256_loadu_si256(ptr as *const __m256i) };

        // Разделяем на младшие и старшие половинки байта
        let lo = _mm256_and_si256(vec, low_mask);
        let hi = _mm256_and_si256(_mm256_srli_epi16(vec, 4), low_mask);

        // Получаем количество единиц для каждой половинки через lookup
        let popcnt_lo = _mm256_shuffle_epi8(lookup, lo);
        let popcnt_hi = _mm256_shuffle_epi8(lookup, hi);

        // Складываем результаты
        let sum = _mm256_add_epi8(popcnt_lo, popcnt_hi);

        // Горизонтальная сумма с использованием SAD (сумма абсолютных разностей)
        let sad = _mm256_sad_epu8(sum, _mm256_setzero_si256());

        // Извлекаем и аккумулируем количество единиц
        let lower = _mm256_extracti128_si256(sad, 0);
        let upper = _mm256_extracti128_si256(sad, 1);
        count += _mm_extract_epi64(lower, 0) as usize;
        count += _mm_extract_epi64(lower, 1) as usize;
        count += _mm_extract_epi64(upper, 0) as usize;
        count += _mm_extract_epi64(upper, 1) as usize;
        ptr = unsafe { ptr.add(32) };
    }

    // Обрабатываем оставшиеся 8-байтные блоки через POPCNT
    while unsafe { ptr.add(8) <= end } {
        use std::arch::x86_64::_popcnt64;

        let chunk = unsafe { (ptr as *const u64).read_unaligned() };
        count += unsafe { _popcnt64(chunk as i64) as usize };
        ptr = unsafe { ptr.add(8) };
    }

    // Обрабатываем оставшиеся байты через lookup table
    while ptr < end {
        let byte = unsafe { *ptr };
        count += BIT_COUNT_TABLE[byte as usize] as usize;
        ptr = unsafe { ptr.add(1) };
    }
    count
}

/// Подсчёт битов с использованием AVX-512 SIMD (512-битные векторы)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw")]
unsafe fn bitcount_avx512_impl(bytes: &[u8]) -> usize {
    let mut count = 0usize;
    let ptr = bytes.as_ptr();
    let end = unsafe { ptr.add(bytes.len()) };
    let mut current = ptr;

    unsafe {
        // Основной цикл по 64 байта
        while current.add(64) <= end {
            use std::arch::x86_64::{
                __m512i, _mm512_loadu_si512, _mm512_popcnt_epi64, _mm512_reduce_add_epi64,
            };

            let vec = _mm512_loadu_si512(current as *const __m512i);
            let popcnt = _mm512_popcnt_epi64(vec);
            count += _mm512_reduce_add_epi64(popcnt) as usize;
            current = current.add(64);
        }

        // Оставшиеся 32 байта
        if current.add(32) <= end {
            use std::arch::x86_64::{
                __m256i, _mm256_extracti128_si256, _mm256_loadu_si256, _mm512_cvtepu8_epi64,
                _mm512_popcnt_epi64, _mm512_reduce_add_epi64,
            };

            let vec = _mm256_loadu_si256(current as *const __m256i);
            let lo = _mm512_cvtepu8_epi64(_mm256_extracti128_si256(vec, 0));
            let hi = _mm512_cvtepu8_epi64(_mm256_extracti128_si256(vec, 1));
            count += _mm512_reduce_add_epi64(_mm512_popcnt_epi64(lo)) as usize;
            count += _mm512_reduce_add_epi64(_mm512_popcnt_epi64(hi)) as usize;
            current = current.add(32);
        }

        // Оставшиеся 8-байтные блоки
        while current.add(8) <= end {
            use std::arch::x86_64::_popcnt64;
            let chunk = (current as *const u64).read_unaligned();
            count += _popcnt64(chunk as i64) as usize;
            current = current.add(8);
        }

        // Последние байты
        while current < end {
            count += BIT_COUNT_TABLE[*current as usize] as usize;
            current = current.add(1);
        }
    }

    count
}

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
