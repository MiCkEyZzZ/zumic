use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::database::{HllDense, HllSparse};

/// Количество регистров в HyperLogLog
/// (обычно степень двойки, здесь 16 384).
const NUM_REGISTERS: usize = 16_384;
/// Ширина каждого регистра в битах (6 бит
/// на регистр для хранения значения rho).
const REGISTER_BITS: usize = 6;
/// Общий размер массива регистров в байтах:
/// NUM_REGISTERS × REGISTER_BITS / 8 = 12 288 байт.
pub const DENSE_SIZE: usize = NUM_REGISTERS * REGISTER_BITS / 8; // 12288 байт

/// Версия формата сериализации для обратной совместимости.
pub const SERIALIZATION_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HllEncoding {
    Sparse(HllSparse),
    Dense(Box<HllDense>),
}

/// HyperLogLog — структура для приближённого
/// подсчёта мощности множества.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hll {
    pub encoding: HllEncoding,
    pub version: u8,
}

/// Статистика использования HLL для мониторинга.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HllStats {
    pub cardinality: f64,
    pub memory_bytes: usize,
    pub is_sparse: bool,
    pub non_zero_registers: usize,
    pub version: u8,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl Hll {
    /// Создаёт новый пустой HLL — все регистры обнулены.
    pub fn new() -> Self {
        Self {
            encoding: HllEncoding::Sparse(HllSparse::new()),
            version: SERIALIZATION_VERSION,
        }
    }

    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            encoding: HllEncoding::Sparse(HllSparse::with_threshold(threshold)),
            version: SERIALIZATION_VERSION,
        }
    }

    /// Добавляет элемент `value` в структуру:
    pub fn add(
        &mut self,
        value: &[u8],
    ) {
        let hash = Self::hash(value);
        let (index, rho) = Self::index_and_rho(hash);

        match &mut self.encoding {
            HllEncoding::Sparse(sparse) => {
                let current = sparse.get_register(index as u16);
                if rho > current {
                    sparse.set_register(index as u16, rho);

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
                zeros = sparse.count_zeros(NUM_REGISTERS);
                sum += zeros as f64 // 2^0 = 1
            }
            HllEncoding::Dense(dense) => {
                // Для dense: итерируем все регистры
                for i in 0..NUM_REGISTERS {
                    let val = dense.get_register(i);
                    if val == 0 {
                        zeros += 1;
                    }
                    sum += 1.0 / (1_u64 << val) as f64;
                }
            }
        }

        let m = NUM_REGISTERS as f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let raw_estimate = alpha * m * m / sum;

        // Коррекция для малых множеств
        if raw_estimate <= 2.5 * m && zeros != 0 {
            m * (m / zeros as f64).ln()
        } else {
            raw_estimate
        }
    }

    /// Объединяет текущий HLL с другими.
    pub fn merge(
        &mut self,
        other: &Hll,
    ) {
        match (&mut self.encoding, &other.encoding) {
            (HllEncoding::Sparse(sparse1), HllEncoding::Sparse(sparse2)) => {
                sparse1.merge(sparse2);
                if sparse1.should_convert_to_dense() {
                    self.convert_to_dense();
                }
            }
            (HllEncoding::Sparse(_), HllEncoding::Dense(_)) => {
                // Если другой dense, сначала конвертируем себя
                self.convert_to_dense();
                self.merge(other);
            }
            (HllEncoding::Dense(dense1), HllEncoding::Sparse(sparse2)) => {
                // Объединяем sparse в dense
                for (index, value) in sparse2.iter() {
                    let current = dense1.get_register(index as usize);
                    if value > current {
                        dense1.set_register(index as usize, value);
                    }
                }
            }
            (HllEncoding::Dense(dense1), HllEncoding::Dense(dense2)) => {
                // Объединяем два dense
                for i in 0..NUM_REGISTERS {
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
            HllEncoding::Sparse(sparse) => HllStats {
                cardinality: self.estimate_cardinality(),
                memory_bytes: sparse.memory_footprint(),
                is_sparse: true,
                non_zero_registers: sparse.len(),
                version: self.version,
            },
            HllEncoding::Dense(_) => {
                let mut non_zero = 0;
                if let HllEncoding::Dense(dense) = &self.encoding {
                    for i in 0..NUM_REGISTERS {
                        if dense.get_register(i) != 0 {
                            non_zero += 1;
                        }
                    }
                }
                HllStats {
                    cardinality: self.estimate_cardinality(),
                    memory_bytes: DENSE_SIZE + std::mem::size_of::<Hll>(),
                    is_sparse: false,
                    non_zero_registers: non_zero,
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
    pub fn is_sparse(&self) -> bool {
        matches!(self.encoding, HllEncoding::Sparse(_))
    }

    /// Хэширует срез байт в 64-битное значение, используя стандартный Hasher.
    fn hash(value: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// Делит 64-битный хэш:
    /// - `index` ← старшие 14 бит (для выбора регистра);
    /// - `rho` ← количество ведущих нулей в оставшихся битах + 1.
    fn index_and_rho(hash: u64) -> (usize, u8) {
        let index = (hash >> (64 - 14)) as usize;
        let remaining = hash << 14 | 1 << 13;
        let rho = remaining.leading_zeros() as u8 + 1;
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
            HllEncoding::Sparse(sparse) => sparse.get_register(index as u16),
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
                sparse.set_register(index as u16, value);
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

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для Hll
////////////////////////////////////////////////////////////////////////////////

impl Default for Hll {
    /// По умолчанию создаёт новый пустой HLL.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_hll_is_sparse() {
        let hll = Hll::new();
        assert!(hll.is_sparse());

        let stats = hll.stats();
        assert!(stats.is_sparse);
        assert_eq!(stats.non_zero_registers, 0);
        assert!(stats.memory_bytes < 1000); // Должно быть намного меньше 12KB
    }

    #[test]
    fn test_add_elements_sparse() {
        let mut hll = Hll::new();

        // Добавляем несколько элементов
        for i in 0..100 {
            hll.add(format!("element_{}", i).as_bytes());
        }

        // Должно остаться sparse
        assert!(hll.is_sparse());

        let cardinality = hll.estimate_cardinality();
        assert!(cardinality > 50.0 && cardinality < 150.0); // Примерная оценка
    }

    #[test]
    fn test_auto_conversion_to_dense() {
        let mut hll = Hll::with_threshold(100);

        // Добавляем много уникальных элементов
        for i in 0..1000 {
            hll.add(format!("element_{}", i).as_bytes());
        }

        // Должно конвертироваться в dense
        assert!(!hll.is_sparse());

        let stats = hll.stats();
        assert!(!stats.is_sparse);
        assert_eq!(stats.memory_bytes, DENSE_SIZE + std::mem::size_of::<Hll>());
    }

    #[test]
    fn test_cardinality_estimation() {
        let mut hll = Hll::new();

        // Добавляем известное кол-во уникальных элементов
        let num_elements = 10000;
        for i in 0..num_elements {
            hll.add(format!("unique_{}", i).as_bytes());
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
        let mut hll1 = Hll::new();
        let mut hll2 = Hll::new();

        // Добавляем разные элементы
        for i in 0..50 {
            hll1.add(format!("a_{}", i).as_bytes());
        }
        for i in 0..50 {
            hll2.add(format!("b_{}", i).as_bytes());
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
        let mut sparse = Hll::with_threshold(50);
        let mut dense = Hll::with_threshold(10);

        // sparse остаётся sparse
        for i in 0..30 {
            sparse.add(format!("sparse_{}", i).as_bytes());
        }

        // dense становится dense
        for i in 0..100 {
            dense.add(format!("dense_{}", i).as_bytes());
        }

        assert!(sparse.is_sparse());
        assert!(!dense.is_sparse());

        sparse.merge(&dense);

        // После merge должно стать dense
        assert!(!sparse.is_sparse());
    }

    #[test]
    fn test_duplicate_elements() {
        let mut hll = Hll::new();

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
        let mut hll = Hll::new();

        for i in 0..100 {
            hll.add(format!("item_{}", i).as_bytes());
        }

        let stats = hll.stats();

        assert!(stats.cardinality > 50.0 && stats.cardinality < 150.0);
        assert!(stats.is_sparse);
        assert!(stats.non_zero_registers > 0);
        assert_eq!(stats.version, SERIALIZATION_VERSION);
        assert!(stats.memory_bytes < DENSE_SIZE);
    }

    #[test]
    fn test_serialization_sparse() {
        let mut hll = Hll::new();

        for i in 0..100 {
            hll.add(format!("test_{}", i).as_bytes());
        }

        let serialized = bincode::serialize(&hll).unwrap();
        let deserialized: Hll = bincode::deserialize(&serialized).unwrap();

        assert_eq!(hll, deserialized);
        assert!(deserialized.is_sparse());
    }

    #[test]
    fn test_serialization_dense() {
        let mut hll = Hll::with_threshold(10);

        for i in 0..1000 {
            hll.add(format!("test_{}", i).as_bytes());
        }

        assert!(!hll.is_sparse());

        let serialized = bincode::serialize(&hll).unwrap();
        let deserialized: Hll = bincode::deserialize(&serialized).unwrap();

        assert_eq!(hll, deserialized);
        assert!(!deserialized.is_sparse());
    }

    #[test]
    fn test_memory_efficiency() {
        let mut sparse = Hll::new();
        let mut dense = Hll::with_threshold(10);

        // Добавляем одинаковое количество элементов
        for i in 0..50 {
            sparse.add(format!("elem_{}", i).as_bytes());
            dense.add(format!("elem_{}", i).as_bytes());
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
}
