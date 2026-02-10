use serde::{Deserialize, Serialize};

use super::{HllHasher, MurmurHasher};
use crate::database::{HllDense, HllSparse};

/// HLL по умолчанию с точностью (P=14, 16K регистров, ~0.81% погрешности)
pub type HllDefault = Hll<14, MurmurHasher>;

/// HLL с компактной конфигурацией (P=4, 16 регистров, ~26% погрешность)
pub type HllCompact = Hll<4, MurmurHasher>;

/// HLL с высокой точностью (P=16, 65K регистров, ~0.5% погрешность)
pub type HllPrecise = Hll<16, MurmurHasher>;

/// HLL с максимальной точностью (P=18, 262K регистров, ~0.4% погрешность)
pub type HllMaxPrecision = Hll<18, MurmurHasher>;

/// Точность HLL по умолчанию (14 бит = 16,384 регистра).
///
/// Стандартная погрешность: ~1.04/sqrt(2^14) ≈ 0.81%
pub const DEFAULT_PRECISION: usize = 14;

/// Минимальная допустимая точность (4 бита = 16 регистров).
///
/// Стандартная погрешность: ~26%
pub const MIN_PRECISION: usize = 4;

/// Максимальная допустимая точность (18 бит = 262,144 регистра).
///
/// Стандартная погрешность: ~0.4%
pub const MAX_PRECISION: usize = 18;

/// Ширина каждого регистра в битах (6 бит
///
/// на регистр для хранения значения rho).
const REGISTER_BITS: usize = 6;

/// Версия формата сериализации для обратной совместимости.
pub const SERIALIZATION_VERSION: u8 = 1;

/// Внутренний трейт для валидации точности на этапе компиляции.
trait ValidatePrecision {
    const IS_VALID: ();
}

/// Представляет HyperLogLog: плотное или разряженное.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HllEncoding<const P: usize> {
    /// Разреженное представление для малых множеств
    Sparse(HllSparse<P>),

    /// Плотное представление для больших множеств
    Dense(Box<HllDense<P>>),
}

/// HyperLogLog — структура для приближённого подсчёта мощности множества с
/// настраиваемой точностью.
///
/// # Дженерик параметр P (Точность)
///
/// - **P ∈ [4..18]** — точность в битах
/// - **P = 4**: 16 регистров, ~26% погрешность, 6 байт
/// - **P = 14** (default): 16,384 регистра, ~0.81% погрешность, 12KB
/// - **P = 18**: 262,144 регистра, ~0.4% погрешность, 196KB
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hll<const P: usize = DEFAULT_PRECISION, H: HllHasher = MurmurHasher> {
    pub encoding: HllEncoding<P>,
    pub version: u8,
    #[serde(skip, default)]
    pub hasher: H, /* ПРИМЕЧАНИЕ: хешер не сериализуется. После десериализации будет
                    * использована функция H::default(). */
}

/// Статистика использования HLL для мониторинга.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HllStats {
    pub cardinality: f64,
    pub memory_bytes: usize,
    pub is_sparse: bool,
    pub non_zero_registers: usize,
    pub precision: usize,
    pub standard_error: f64,
    pub hasher_name: &'static str,
    pub version: u8,
}

/// Builder для создания HLL с различными параметрами.
pub struct HllBuilder<const P: usize = DEFAULT_PRECISION, H: HllHasher = MurmurHasher> {
    threshold: Option<usize>,
    hasher: H,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl<const P: usize, H: HllHasher> Hll<P, H> {
    /// Создаёт новый пустой HLL с заданной точностью.
    pub fn new() -> Self {
        let _: () = Self::IS_VALID;

        Self {
            encoding: HllEncoding::Sparse(HllSparse::new()),
            version: SERIALIZATION_VERSION,
            hasher: H::default(),
        }
    }

    /// Создаёт HLL с заданным хешером.
    pub fn with_hasher(hasher: H) -> Self {
        let _: () = Self::IS_VALID;

        Self {
            encoding: HllEncoding::Sparse(HllSparse::new()),
            version: SERIALIZATION_VERSION,
            hasher,
        }
    }

    /// Создаёт HLL с заданным порогом конверсии sparse->dense.
    pub fn with_threshold(threshold: usize) -> Self {
        let _: () = Self::IS_VALID;

        Self {
            encoding: HllEncoding::Sparse(HllSparse::with_threshold(threshold)),
            version: SERIALIZATION_VERSION,
            hasher: H::default(),
        }
    }

    /// Возвращает точность HLL (кол-во бит для индекса регистра).
    #[inline]
    pub const fn precision(&self) -> usize {
        P
    }

    /// Возвращает кол-во регистров.
    #[inline]
    pub const fn num_registers(&self) -> usize {
        num_registers(P)
    }

    /// Вовзращает размер dense представления в байтах.
    #[inline]
    pub const fn dense_size(&self) -> usize {
        dense_size(P)
    }

    /// Вовзращает стандартную погрешность для текущей точности.
    #[inline]
    pub fn standard_error(&self) -> f64 {
        standard_error(P)
    }

    /// Возвращает имя используемого хешера.
    #[inline]
    pub fn hasher_name(&self) -> &'static str {
        self.hasher.name()
    }

    /// Добавляет элемент `value` в структуру HLL
    pub fn add(
        &mut self,
        value: &[u8],
    ) {
        let hash = self.hasher.hash_bytes(value);
        let (index, rho) = Self::index_and_rho(hash);

        match &mut self.encoding {
            HllEncoding::Sparse(sparse) => {
                let current = sparse.get_register(index);
                if rho > current {
                    sparse.set_register(index, rho);

                    // Проверяем, нужно ли переключиться на dense
                    if sparse.should_convert_to_dense() {
                        self.convert_to_dense();
                    }
                }
            }
            HllEncoding::Dense(dense) => {
                let current = dense.get_register(index);
                if rho > current {
                    dense.set_register(index, rho);
                }
            }
        }
    }

    /// Оценивает кардинальность множества (количество уникальных элементов).
    pub fn estimate_cardinality(&self) -> f64 {
        let mut sum = 0.0;
        let mut zeros = 0;

        match &self.encoding {
            HllEncoding::Sparse(sparse) => {
                // Для sparse: итерируем только ненулевые регистры
                for (_index, val) in sparse.iter() {
                    sum += 1.0 / (1_u64 << val) as f64;
                }

                // Добавляем вклад нулевых регистров
                zeros = sparse.count_zeros(self.num_registers());
                sum += zeros as f64 // 2^0 = 1
            }
            HllEncoding::Dense(dense) => {
                // Для dense: итерируем все регистры
                for i in 0..self.num_registers() {
                    let val = dense.get_register(i);
                    if val == 0 {
                        zeros += 1;
                    }
                    sum += 1.0 / (1_u64 << val) as f64;
                }
            }
        }

        let m = self.num_registers() as f64;
        let alpha = alpha_constant(P);
        let raw_estimate = alpha * m * m / sum;

        // Коррекция для малых множеств
        if raw_estimate <= 2.5 * m && zeros != 0 {
            m * (m / zeros as f64).ln()
        } else {
            raw_estimate
        }
    }

    /// Объединяет текущий HLL с другими.
    ///
    /// **ВАЖНО**: Оба HLL должны иметь одинаковый тип хешера `H`.
    /// Это гарантирует типовой системой: `merge(&mut self, other: &Hll<P, H>)`.
    pub fn merge(
        &mut self,
        other: &Hll<P, H>,
    ) {
        let num_registers = self.num_registers();

        match (&mut self.encoding, &other.encoding) {
            (HllEncoding::Sparse(sparse1), HllEncoding::Sparse(sparse2)) => {
                sparse1.merge(sparse2);
                if sparse1.should_convert_to_dense() {
                    self.convert_to_dense();
                }
            }
            (HllEncoding::Sparse(_), HllEncoding::Dense(_)) => {
                self.convert_to_dense();
                self.merge(other);
            }
            (HllEncoding::Dense(dense1), HllEncoding::Sparse(sparse2)) => {
                for (index, value) in sparse2.iter() {
                    let current = dense1.get_register(index);
                    if value > current {
                        dense1.set_register(index, value);
                    }
                }
            }
            (HllEncoding::Dense(dense1), HllEncoding::Dense(dense2)) => {
                for i in 0..num_registers {
                    let val1 = dense1.get_register(i);
                    let val2 = dense2.get_register(i);
                    if val2 > val1 {
                        dense1.set_register(i, val2);
                    }
                }
            }
        }
    }

    /// Возвращает статистику использования HLL.
    pub fn stats(&self) -> HllStats {
        match &self.encoding {
            HllEncoding::Sparse(sparse) => {
                let heap = sparse.memory_footprint();
                HllStats {
                    cardinality: self.estimate_cardinality(),
                    memory_bytes: std::mem::size_of::<Hll<P, H>>().saturating_add(heap),
                    is_sparse: true,
                    non_zero_registers: sparse.len(),
                    precision: P,
                    standard_error: self.standard_error(),
                    hasher_name: self.hasher_name(),
                    version: self.version,
                }
            }
            HllEncoding::Dense(dense) => {
                let mut non_zero = 0;
                for i in 0..self.num_registers() {
                    if dense.get_register(i) != 0 {
                        non_zero += 1;
                    }
                }

                let heap = dense.memory_footprint();
                HllStats {
                    cardinality: self.estimate_cardinality(),
                    memory_bytes: std::mem::size_of::<Hll<P, H>>().saturating_add(heap),
                    is_sparse: false,
                    non_zero_registers: non_zero,
                    precision: P,
                    standard_error: self.standard_error(),
                    hasher_name: self.hasher_name(),
                    version: self.version,
                }
            }
        }
    }

    pub fn convert_to_dense(&mut self) {
        if let HllEncoding::Sparse(sparse) = &self.encoding {
            let dense = HllDense::from_sparse(sparse);
            self.encoding = HllEncoding::Dense(Box::new(dense));
        }
    }

    /// Проверяем, использует ли HLL sparse представление.
    #[inline]
    pub fn is_sparse(&self) -> bool {
        matches!(self.encoding, HllEncoding::Sparse(_))
    }

    /// Делит 64-битный хэш:
    ///
    /// - `index` ← старшие 14 бит (для выбора регистра);
    /// - `rho` ← количество ведущих нулей в оставшихся битах + 1.
    fn index_and_rho(hash: u64) -> (usize, u8) {
        // Старшие Р бит индекс регистра
        let index = (hash >> (64 - P)) as usize;

        // Оставшиеся (64 - Р) бит
        let w = hash << P;

        // rho = число ведущих нулей + 1
        // но НЕ больше (64 - Р + 1)
        let rho = ((w.leading_zeros() + 1).min((64 - P) as u32 + 1)) as u8;

        (index, rho)
    }

    // NOTE: методы `get_register` и `set_register` в данный момент не используются.
    // Они сохранены как внутренние всп. ф-ии для возможной унификации логики
    // доступа к Sparse/Dense в будущем.

    /// Считывает 6-битный регистр под номером `index`.
    #[allow(dead_code)]
    fn get_register(
        &self,
        index: usize,
    ) -> u8 {
        match &self.encoding {
            HllEncoding::Sparse(sparse) => sparse.get_register(index),
            HllEncoding::Dense(dense) => dense.get_register(index),
        }
    }

    /// Записывает 6-битное значение `value` в регистр `index`.
    #[allow(dead_code)]
    fn set_register(
        &mut self,
        index: usize,
        value: u8,
    ) {
        match &mut self.encoding {
            HllEncoding::Sparse(sparse) => {
                sparse.set_register(index, value);
                if sparse.should_convert_to_dense() {
                    self.convert_to_dense();
                }
            }
            HllEncoding::Dense(dense) => {
                dense.set_register(index, value);
            }
        }
    }
}

impl<const P: usize, H: HllHasher> HllBuilder<P, H> {
    /// Создаёт новый builder
    pub fn new() -> Self {
        Self {
            threshold: None,
            hasher: H::default(),
        }
    }

    /// Создаёт builder с заданным хешером.
    pub fn with_hasher(hasher: H) -> Self {
        Self {
            threshold: None,
            hasher,
        }
    }

    /// Устанавливаем порого конверсии sparse->dense
    pub fn threshold(
        mut self,
        threshold: usize,
    ) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Строит HLL с заданными параметрами.
    pub fn build(self) -> Hll<P, H> {
        match self.threshold {
            Some(t) => Hll {
                encoding: HllEncoding::Sparse(HllSparse::with_threshold(t)),
                version: SERIALIZATION_VERSION,
                hasher: self.hasher,
            },
            None => Hll {
                encoding: HllEncoding::Sparse(HllSparse::new()),
                version: SERIALIZATION_VERSION,
                hasher: self.hasher,
            },
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для Hll, HllBuilder
////////////////////////////////////////////////////////////////////////////////

impl<const P: usize, H: HllHasher> ValidatePrecision for Hll<P, H> {
    const IS_VALID: () = assert!(
        P >= MIN_PRECISION && P <= MAX_PRECISION,
        "HLL precision must be in range [4..18]"
    );
}

impl<const P: usize, H: HllHasher> Default for Hll<P, H> {
    /// По умолчанию создаёт новый пустой HLL.
    fn default() -> Self {
        Self::new()
    }
}

impl<const P: usize, H: HllHasher> Default for HllBuilder<P, H> {
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Внешние методы и функции
////////////////////////////////////////////////////////////////////////////////

/// Вычисляет кол-во регистров для заданной точности.
///
/// - P=4 -> 16 регистров
/// - P=14 -> 16,384 регистра (по умолчанию)
/// - P=17 -> 262,144 регистра
#[inline]
pub const fn num_registers(precision: usize) -> usize {
    1 << precision
}

/// Вычисляет размер dense массива в байтах для заданной точности.
#[inline]
pub const fn dense_size(precision: usize) -> usize {
    num_registers(precision) * REGISTER_BITS / 8
}

/// Вычисляет стандартную погрешность для заданной точности.
///
/// Формула: σ = 1.04 / sqrt(m), где m = 2^precision
pub fn standard_error(precision: usize) -> f64 {
    1.04 / (num_registers(precision) as f64).sqrt()
}

/// Вычисляет альфу константу для заданной точности.
///
/// α_m = 0.7213 / (1 + 1.079/m) для m ≥ 128
pub fn alpha_constant(precision: usize) -> f64 {
    let m = num_registers(precision) as f64;
    if precision >= 7 {
        0.7213 / (1.0 + 1.079 / m) // m >= 128
    } else if precision == 4 {
        0.673
    } else if precision == 5 {
        0.697
    } else {
        0.709 // precision == 6
    }
}

/// Выбирает оптимальную точность на основе ожидаемой кардинальности и целевой
/// погрешности.
pub fn choose_precision(
    _expected_cardinality: u64,
    target_error: f64,
) -> usize {
    let required_registers = (1.04 / target_error).powi(2);

    (required_registers.log2().ceil() as usize).clamp(MIN_PRECISION, MAX_PRECISION)
}

/// Вычисляет рекомендуемый sparse threshold для заданной точности.
///
/// Эвристика: threshold = min(num_register / 4, 3000)
pub fn recommended_threshold(precision: usize) -> usize {
    let num_regs = num_registers(precision);
    (num_regs / 4).min(3000)
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{SipHasher, XxHasher};

    type H = HllDefault;

    #[test]
    fn test_new_hll_is_sparse() {
        let hll = H::new();
        assert!(hll.is_sparse());

        let stats = hll.stats();
        assert!(stats.is_sparse);
        assert_eq!(stats.non_zero_registers, 0);
        assert!(stats.memory_bytes < 1000); // Должно быть намного меньше 12KB
        assert_eq!(stats.hasher_name, "MurmurHash");
    }

    #[test]
    fn test_different_hashers() {
        let mut hll_murmur = Hll::<14, MurmurHasher>::new();
        let mut hll_xxhash = Hll::<14, XxHasher>::new();
        let mut hll_siphash = Hll::<14, SipHasher>::new();

        // Добавляем одинаковые данные
        for i in 0..1000 {
            let data = format!("item_{i}");
            hll_murmur.add(data.as_bytes());
            hll_xxhash.add(data.as_bytes());
            hll_siphash.add(data.as_bytes());
        }

        // Все должны давать близкие оценки
        let est_murmur = hll_murmur.estimate_cardinality();
        let est_xxhash = hll_xxhash.estimate_cardinality();
        let est_siphash = hll_siphash.estimate_cardinality();

        assert!((est_murmur - 1000.0).abs() < 100.0);
        assert!((est_xxhash - 1000.0).abs() < 100.0);
        assert!((est_siphash - 1000.0).abs() < 100.0);

        // Проверяем имена хеширов
        assert_eq!(hll_murmur.hasher_name(), "MurmurHash");
        assert_eq!(hll_xxhash.hasher_name(), "XxHasher");
        assert_eq!(hll_siphash.hasher_name(), "SipHash");
    }

    #[test]
    fn test_builder_with_hasher() {
        let xxhasher = XxHasher::default();
        let hll = HllBuilder::<14, XxHasher>::with_hasher(xxhasher)
            .threshold(5000)
            .build();

        assert_eq!(hll.hasher_name(), "XxHasher");
        assert!(hll.is_sparse());
    }

    #[test]
    fn test_precision_constants() {
        assert_eq!(num_registers(4), 16);
        assert_eq!(num_registers(14), 16_384);
        assert_eq!(num_registers(18), 262_144);

        assert_eq!(dense_size(4), 12); // 16 * 6 / 8 = 12 байтов
        assert_eq!(dense_size(14), 12_288); // 16384 * 6 / 8 = 12288 байтов
        assert_eq!(dense_size(18), 196_608); // 262144 * 6 / 8 = 196608 байтов
    }

    #[test]
    fn test_standard_error_calculation() {
        let err4 = standard_error(4);
        let err14 = standard_error(14);
        let err18 = standard_error(18);

        // P=4; 1.04 / sqrt(16) = 0.26 = 26%
        assert!((err4 - 0.26).abs() < 0.01);

        // P=14: 1.04/sqrt(16384) ≈ 0.00812 = 0.81%
        assert!((err14 - 0.00812).abs() < 0.0001);

        // P=18: 1.04/sqrt(262144) ≈ 0.00203 = 0.2%
        assert!((err18 - 0.00203).abs() < 0.0001);
    }

    #[test]
    fn test_different_precisions() {
        let n = 200_000;

        let mut hll4 = Hll::<4>::new();
        let mut hll14 = Hll::<14>::new();
        let mut hll18 = Hll::<18>::new();

        // Добавляем одинаковые элементы
        for i in 0..n {
            let elem = format!("item_{i}");
            hll4.add(elem.as_bytes());
            hll14.add(elem.as_bytes());
            hll18.add(elem.as_bytes());
        }

        let error4 = (hll4.estimate_cardinality() - n as f64).abs() / n as f64;
        let error14 = (hll14.estimate_cardinality() - n as f64).abs() / n as f64;
        let error18 = (hll18.estimate_cardinality() - n as f64).abs() / n as f64;

        assert!(error18 < error14);
        assert!(error14 < error4);
    }

    #[test]
    fn test_precision_error_bounds() {
        assert!(standard_error(18) < standard_error(14));
        assert!(standard_error(14) < standard_error(4));
    }

    #[test]
    fn test_type_aliases() {
        let _compact: HllCompact = Hll::<4>::new();
        let _default: HllDefault = Hll::<14>::new();
        let _precise: HllPrecise = Hll::<16>::new();
        let _max: HllMaxPrecision = Hll::<18>::new();
    }

    #[test]
    fn test_builder_pattern() {
        let hll1 = HllBuilder::<14>::new().build();
        let hll2 = HllBuilder::<14>::new().threshold(5000).build();

        assert!(hll1.is_sparse());
        assert!(hll2.is_sparse());
    }

    #[test]
    fn test_stats_include_precision() {
        let mut hll = Hll::<14>::new();

        for i in 0..100 {
            hll.add(format!("item_{i}").as_bytes());
        }

        let stats = hll.stats();

        assert_eq!(stats.precision, 14);
        assert!((stats.standard_error - 0.00812).abs() < 0.0001);
    }

    #[test]
    fn test_memory_scaling_with_precision() {
        let hll4 = Hll::<4>::new();
        let hll14 = Hll::<14>::new();
        let hll18 = Hll::<18>::new();

        // Плотные размеры должны масштабироваться экспоненциально
        assert_eq!(hll4.dense_size(), 12); // 6 байт разреженный
        assert_eq!(hll14.dense_size(), 12_288); // 12 КБ
        assert_eq!(hll18.dense_size(), 196_608); // 196 КБ
    }

    #[test]
    fn test_add_elements_sparse() {
        let mut hll = H::new();

        // Добавляем несколько элементов
        for i in 0..100 {
            hll.add(format!("element_{i}").as_bytes());
        }

        // Должно остаться sparse
        assert!(hll.is_sparse());

        let cardinality = hll.estimate_cardinality();
        assert!(cardinality > 50.0 && cardinality < 150.0); // Примерная оценка
    }

    #[test]
    fn test_auto_conversion_to_dense() {
        let mut hll = H::with_threshold(100);

        for i in 0..1000 {
            hll.add(format!("element_{i}").as_bytes());
        }

        assert!(!hll.is_sparse());

        let stats = hll.stats();
        assert!(!stats.is_sparse);

        let min_expected = std::mem::size_of::<Hll<14, MurmurHasher>>() + hll.dense_size();

        assert!(
            stats.memory_bytes >= min_expected,
            "memory_bytes={}, min_expected={}",
            stats.memory_bytes,
            min_expected
        );
    }

    #[test]
    fn test_cardinality_estimation() {
        let mut hll = H::new();

        // Добавляем известное кол-во уникальных элементов
        let num_elements = 10000;
        for i in 0..num_elements {
            hll.add(format!("unique_{i}").as_bytes());
        }

        let estimate = hll.estimate_cardinality();
        let error = (estimate - num_elements as f64).abs() / num_elements as f64;

        // Стандартная погрешность HLL ~1.04/sqrt(m) ~ 0.81% для m = 16384
        // Проверяем, что ошибка в пределах 5%
        assert!(
            error < 0.05,
            "Error: {:.2}%, estimate: {}",
            error * 100.0,
            estimate
        );
    }

    #[test]
    fn test_merge_sparse_sparse() {
        let mut hll1 = H::new();
        let mut hll2 = H::new();

        // Добавляем разные элементы
        for i in 0..50 {
            hll1.add(format!("a_{i}").as_bytes());
        }
        for i in 0..50 {
            hll2.add(format!("b_{i}").as_bytes());
        }

        let card1 = hll1.estimate_cardinality();
        let card2 = hll2.estimate_cardinality();

        hll1.merge(&hll2);
        let card_merged = hll1.estimate_cardinality();

        // Объединённая кардинальность должна быть примерно равна сумме
        assert!(card_merged > card1 && card_merged > card2);
        assert!((card_merged - (card1 + card2)).abs() < 20.0);
    }

    #[test]
    fn test_merge_sparse_dense() {
        let mut sparse = H::with_threshold(50);
        let mut dense = H::with_threshold(10);

        // sparse остаётся sparse
        for i in 0..30 {
            sparse.add(format!("sparse_{i}").as_bytes());
        }

        // dense становится dense
        for i in 0..100 {
            dense.add(format!("dense_{i}").as_bytes());
        }

        assert!(sparse.is_sparse());
        assert!(!dense.is_sparse());

        sparse.merge(&dense);

        // После merge должно стать dense
        assert!(!sparse.is_sparse());
    }

    #[test]
    fn test_duplicate_elements() {
        let mut hll = H::new();

        // Добавляем одинаковые элементы
        for _ in 0..1000 {
            hll.add(b"same_element");
        }

        let estimate = hll.estimate_cardinality();

        // Должна оценить ~1 уникальный элемент
        assert!(estimate < 5.0);
    }

    #[test]
    fn test_stats() {
        let mut hll = H::new();

        for i in 0..100 {
            hll.add(format!("item_{i}").as_bytes());
        }

        let stats = hll.stats();

        assert!(stats.cardinality > 50.0 && stats.cardinality < 150.0);
        assert!(stats.is_sparse);
        assert!(stats.non_zero_registers > 0);
        assert_eq!(stats.version, SERIALIZATION_VERSION);
        assert!(stats.memory_bytes < hll.dense_size());
    }

    #[test]
    fn test_serialization_sparse() {
        let mut hll = H::new();

        for i in 0..100 {
            hll.add(format!("test_{i}").as_bytes());
        }

        let serialized = bincode::serialize(&hll).unwrap();
        let deserialized: H = bincode::deserialize(&serialized).unwrap();

        assert_eq!(hll, deserialized);
        assert!(deserialized.is_sparse());
    }

    #[test]
    fn test_serialization_dense() {
        let mut hll = H::with_threshold(10);

        for i in 0..1000 {
            hll.add(format!("test_{i}").as_bytes());
        }

        assert!(!hll.is_sparse());

        let serialized = bincode::serialize(&hll).unwrap();
        let deserialized: H = bincode::deserialize(&serialized).unwrap();

        assert_eq!(hll, deserialized);
        assert!(!deserialized.is_sparse());
    }

    #[test]
    fn test_memory_efficiency() {
        let mut sparse = H::new();
        let mut dense = H::with_threshold(10);

        // Добавляем одинаковое количество элементов
        for i in 0..50 {
            sparse.add(format!("elem_{i}").as_bytes());
            dense.add(format!("elem_{i}").as_bytes());
        }

        // собираем статистику
        let sparse_stats = sparse.stats();
        let dense_stats = dense.stats();

        // проверки
        assert!(sparse_stats.is_sparse, "expected sparse to remain sparse");
        assert!(!dense_stats.is_sparse, "expected dense to be dense");

        assert!(
            sparse_stats.memory_bytes < dense_stats.memory_bytes,
            "Expected sparse ({}) < dense ({}). If you want a stronger guarantee \
            (e.g. 10x), increase the number of unique elements inserted in the test \
            or optimize sparse.memory_footprint().",
            sparse_stats.memory_bytes,
            dense_stats.memory_bytes
        );
    }

    #[test]
    fn test_memory_monotonic_growth() {
        let mut hll = Hll::<14>::with_threshold(50);
        let mut last_mem = hll.stats().memory_bytes;

        for i in 0..200 {
            hll.add(format!("elem_{i}").as_bytes());
            let mem = hll.stats().memory_bytes;
            assert!(mem >= last_mem);
            last_mem = mem;
        }
    }

    #[test]
    fn test_merge_idempotent() {
        let mut hll = Hll::<14>::new();

        for i in 0..1000 {
            hll.add(format!("x_{i}").as_bytes());
        }

        let before = hll.clone();
        hll.merge(&before);

        assert_eq!(hll, before);
    }

    #[test]
    fn test_clone_preserves_memory_footprint() {
        let mut hll = Hll::<14>::with_threshold(50);

        for i in 0..500 {
            hll.add(format!("e_{i}").as_bytes());
        }

        let clone = hll.clone();

        assert_eq!(hll.stats().memory_bytes, clone.stats().memory_bytes);
    }

    #[test]
    fn test_merge_with_same_hasher() {
        let mut hll1 = Hll::<14, MurmurHasher>::new();
        let mut hll2 = Hll::<14, MurmurHasher>::new();

        for i in 0..50 {
            hll1.add(format!("a_{i}").as_bytes());
        }
        for i in 0..50 {
            hll2.add(format!("b_{i}").as_bytes());
        }

        let card1 = hll1.estimate_cardinality();
        let card2 = hll2.estimate_cardinality();

        hll1.merge(&hll2);
        let card_merged = hll1.estimate_cardinality();

        assert!(card_merged > card1 && card_merged > card2);
        assert!((card_merged - (card1 + card2)).abs() < 20.0);
    }

    #[test]
    fn test_stats_includes_hasher_name() {
        let hll = Hll::<14, XxHasher>::new();
        let stats = hll.stats();

        assert_eq!(stats.hasher_name, "XxHasher");
        assert_eq!(stats.precision, 14);
    }

    #[test]
    fn test_serialization_preserves_functionality() {
        let mut hll = H::new();

        for i in 0..100 {
            hll.add(format!("test_{i}").as_bytes());
        }

        let serialized = bincode::serialize(&hll).unwrap();
        let mut deserialized: H = bincode::deserialize(&serialized).unwrap();

        // После десериализации хешер восстанавливается через Default
        assert_eq!(deserialized.hasher_name(), "MurmurHash");

        // Можем продолжать добавлять элементы
        deserialized.add(b"new_item");
        assert!(deserialized.estimate_cardinality() > 100.0);
    }
}
