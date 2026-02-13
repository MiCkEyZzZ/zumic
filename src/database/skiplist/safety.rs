/// Макрос для debug-time проверки инвариантов.
///
/// В release-сборках компилируется в no-op.
#[macro_export]
macro_rules! debug_assert_invariant {
    ($cond:expr, $($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            if !$cond {
                panic!("Invariant violation: {}", format!($($arg)*));
            }
        }
    };
}

/// Макрос для валидации условий с возвратом ошибки.
#[macro_export]
macro_rules! validate {
    ($cond:expr, $err:expr) => {
        if !$cond {
            return Err($err);
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Уровень узла превышает максимальный
    InvalidLevel { node_level: usize, max_level: usize },
    /// Размер вектора forward не соответствует уровню
    ForwardVectorMismatch { expected: usize, actual: usize },
    /// Нарушен порядок сортировки
    SortOrderViolation { message: String },
    /// Длина списка не соответствует реальному кол-ву узлов
    LengthMismatch { expected: usize, actual: usize },
    /// Обнаружена циклическая ссылка
    CyclicReference { message: String },
    /// Backward-ссылка указывает на неверный узел
    InvalidBackwardLink { message: String },
}

/// Статистика структуры SkipList.
#[derive(Debug, Clone, PartialEq)]
pub struct SkipListStatistics {
    /// Количество узлов
    pub node_count: usize,
    /// Распределение по уровням
    pub level_distribution: Vec<usize>,
    /// Текущий максимальный уровень
    pub current_max_level: usize,
    /// Максимально возможный уровень
    pub max_possible_level: usize,
    /// Средний уровень узла
    pub average_level: f64,
}

impl SkipListStatistics {
    /// Создает пустую статистику.
    pub fn empty(max_level: usize) -> Self {
        Self {
            node_count: 0,
            level_distribution: vec![0; max_level],
            current_max_level: 1,
            max_possible_level: max_level,
            average_level: 0.0,
        }
    }

    /// Вычисляет средний уровень.
    pub fn compute_average_level(&mut self) {
        if self.node_count == 0 {
            self.average_level = 0.0;
            return;
        }

        let total_levels: usize = self
            .level_distribution
            .iter()
            .enumerate()
            .map(|(level, &count)| (level + 1) * count)
            .sum();

        self.average_level = total_levels as f64 / self.node_count as f64;
    }

    /// Форматирует статистику для вывода.
    pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str("SkipList Statistics:\n");
        report.push_str(&format!("  Total nodes: {}\n", self.node_count));
        report.push_str(&format!(
            "  Current max level: {}\n",
            self.current_max_level
        ));
        report.push_str(&format!(
            "  Max possible level: {}\n",
            self.max_possible_level
        ));
        report.push_str(&format!("  Average level: {:.2}\n", self.average_level));
        report.push_str("  Level distribution:\n");

        for (level, &count) in self.level_distribution.iter().enumerate() {
            if count > 0 {
                let percentage = (count as f64 / self.node_count as f64) * 100.0;
                report.push_str(&format!(
                    "    Level {}: {} nodes ({:.1}%)\n",
                    level + 1,
                    count,
                    percentage
                ));
            }
        }

        report
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            ValidationError::InvalidLevel {
                node_level,
                max_level,
            } => {
                write!(
                    f,
                    "Node level {node_level} exceeds maximum level {max_level}"
                )
            }
            ValidationError::ForwardVectorMismatch { expected, actual } => {
                write!(
                    f,
                    "Forward vector size mismatch: expected {expected}, got {actual}"
                )
            }
            ValidationError::SortOrderViolation { message } => {
                write!(f, "Sort order violation: {message}")
            }
            ValidationError::LengthMismatch { expected, actual } => {
                write!(f, "Length mismatch: expected {expected}, got {actual}")
            }
            ValidationError::CyclicReference { message } => {
                write!(f, "Cyclic reference detected: {message}")
            }
            ValidationError::InvalidBackwardLink { message } => {
                write!(f, "Invalid backward link: {message}")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statistics_empty() {
        let stats = SkipListStatistics::empty(16);

        assert_eq!(stats.node_count, 0);
        assert_eq!(stats.current_max_level, 1);
        assert_eq!(stats.average_level, 0.0);
    }

    #[test]
    fn test_statistics_compute_average() {
        let mut stats = SkipListStatistics {
            node_count: 3,
            level_distribution: vec![1, 1, 1, 0],
            current_max_level: 3,
            max_possible_level: 4,
            average_level: 0.0,
        };

        stats.compute_average_level();
        assert_eq!(stats.average_level, 2.0); // (1*1 + 2*1 + 3*1) / 3 = 2.0
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::InvalidLevel {
            node_level: 20,
            max_level: 16,
        };
        assert!(err.to_string().contains("exceeds maximum level"));
    }
}
