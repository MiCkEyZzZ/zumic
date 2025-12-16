/// Результат валидации AOF записи.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Запись валидна
    Valid,
    /// Запись повреждена (checksum не сходится)
    Corrupted { expected: u32, actual: u32 },
    /// Запись обрезана (недостаточно данных)
    Truncated,
    /// Неизвестный тип операции
    UnknownOperation(u8),
    /// Неожиданный конец файла при чтении длины
    UnexpectedEof,
}

/// Режим repair для AOF файлов.
#[derive(Debug, Clone, Copy)]
pub enum RepairMode {
    /// Пропускать повреждённые записи
    Skip,
    /// Остановиться на первой ошибке
    Strict,
    /// Попытаться восстановить обрезанные записи
    Recover,
}

/// CRC32 implementation для checksumming AOF записей.
/// Использует стандартный IEEE 802.3 полином для совместимости.
pub struct Crc32 {
    table: [u32; 256],
}

/// Статистика integrity проверки.
#[derive(Debug, Default, Clone)]
pub struct IntegrityStats {
    /// Общее кол-во проверенных записей
    pub records_checked: usize,
    /// Кол-во валидных записей
    pub records_valid: usize,
    /// Кол-во повреждённых записей
    pub records_corrupted: usize,
    /// Кол-во обрезанных записей
    pub records_truncated: usize,
    /// Кол-во записей с неизвестными операциями
    pub records_unknown_op: usize,
    /// Кол-во пропущенных записей
    pub records_skipped: usize,
}

/// Валидатор для проверки целостности AOF записей.
pub struct AofValidator {
    crc32: Crc32,
    stats: IntegrityStats,
}

/// Результат repair операций.
#[derive(Debug)]
pub struct RepairResult {
    /// Статистика integrity
    pub stats: IntegrityStats,
    /// Был ли файл изменён
    pub modified: bool,
    /// Сообщения об ошибках и восстановлении
    pub messages: Vec<String>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl Crc32 {
    /// IEEE 802.3 полином для CRC32.
    const POLYNOMIAL: u32 = 0xEDB88320;

    /// Создаёт новый экземпляр CRC32 с предвычесленной таблицей для быстрого
    /// вычисления CRC32 по стандартному полиному IEEE 802.3.
    ///
    /// # Возвращает:
    /// - `Crc32` - объект CRC32 с заполненной таблицей
    pub fn new() -> Self {
        let mut table = [0u32; 256];

        for (i, slot) in table.iter_mut().enumerate() {
            let mut crc = i as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ Self::POLYNOMIAL;
                } else {
                    crc >>= 1;
                }
            }
            *slot = crc
        }

        Self { table }
    }

    /// Вычисляет CRC32 для переданных данных.
    ///
    /// # Возвращает:
    /// - `u32` - вычисленное значение CRC32
    pub fn checksum(
        &self,
        data: &[u8],
    ) -> u32 {
        let mut crc = 0xFFFFFFFF;

        for &byte in data {
            let table_index = ((crc ^ byte as u32) & 0xFF) as usize;
            crc = (crc >> 8) ^ self.table[table_index];
        }

        crc ^ 0xFFFFFFFF
    }

    /// Проверяет CRC32 для переданных данных.
    ///
    /// # Возвращает:
    /// - `bool` - `true`, если вычисленное значение совпадает с ожидаемым,
    ///   иначе `false`
    pub fn verify(
        &self,
        data: &[u8],
        expected: u32,
    ) -> bool {
        self.checksum(data) == expected
    }
}

impl IntegrityStats {
    /// Обновляет статистику на основе результата валидации AOF записи.
    pub fn update(
        &mut self,
        result: &ValidationResult,
    ) {
        self.records_checked += 1;

        match result {
            ValidationResult::Valid => self.records_valid += 1,
            ValidationResult::Corrupted { .. } => self.records_corrupted += 1,
            ValidationResult::Truncated => self.records_truncated += 1,
            ValidationResult::UnknownOperation(_) => self.records_unknown_op += 1,
            ValidationResult::UnexpectedEof => {
                // EOF считаем как truncated
                self.records_truncated += 1;
            }
        }
    }

    /// Помечает запись как пропущенную.
    pub fn skip_record(&mut self) {
        self.records_skipped += 1;
    }

    /// Возвращает процент успешно проверенных записей.
    ///
    /// # Возвращает:
    /// - `f64` - процент валидных записей (0..100)
    pub fn success_rate(&self) -> f64 {
        if self.records_checked == 0 {
            return 100.0;
        }
        (self.records_valid as f64 / self.records_checked as f64) * 100.0
    }

    /// Проверяет, есть ли критические проблемы с целостностью.
    ///
    /// Критическая проблема считается, если более 5% записей повреждены.
    ///
    /// # Возвращает:
    /// - `true`, если есть критические проблемы
    /// - `false`, если нет критической проблемы
    pub fn has_critical_issues(&self) -> bool {
        if self.records_checked == 0 {
            return false;
        }
        let corruption_rate = self.records_corrupted as f64 / self.records_checked as f64 * 100.0;
        corruption_rate > 5.0
    }
}

impl AofValidator {
    /// Создаёт новый валидатор для проверки AOF записей.
    ///
    /// # Возвращает:
    /// - `AofValidator` - объект валидатора с предвычесленным CRC32 и пустой
    ///   статистикой
    pub fn new() -> Self {
        Self {
            crc32: Crc32::new(),
            stats: IntegrityStats::default(),
        }
    }

    /// Помечает текущую запись как пропущенную.
    ///
    /// Используется для режимов replay `skip` или `log`.
    /// Увеличивает счётчик `records_skipped`.
    pub fn mark_skipped(&mut self) {
        self.stats.skip_record();
    }

    /// Валидирует одну AOF запись с CRC32.
    ///
    /// Ожидаемый формат записи: `[op][checksum][key_len][key][val_len?][val?]`
    ///
    /// # Возвращает:
    /// - `ValidationResult` - результат проверки записи
    pub fn validate_record(
        &mut self,
        data: &[u8],
    ) -> ValidationResult {
        let result = self.validate_record_internal(data);
        self.stats.update(&result);
        result
    }

    /// Возвращает текущую статистику валидатора.
    ///
    /// # Возвращает:
    /// - `&IntegrityStats` - ссылка на внутреннюю статистику
    pub fn stats(&self) -> &IntegrityStats {
        &self.stats
    }

    /// Сбрасывает текущую статистику.
    pub fn reset_stats(&mut self) {
        self.stats = IntegrityStats::default();
    }

    /// Вычисляет CRC32 для переданного payload.
    ///
    /// # Возвращает:
    /// - `u32` - вычисленное значение CRC32
    pub fn compute_checksum(
        &self,
        payload: &[u8],
    ) -> u32 {
        self.crc32.checksum(payload)
    }

    /// Внутренняя проверка одной AOF записи.
    ///
    /// Используется для реализации `validate_record`.
    fn validate_record_internal(
        &self,
        data: &[u8],
    ) -> ValidationResult {
        // Минимальный размер: op(1) + checksum(4) + key_len(4) = 9 байт
        if data.len() < 9 {
            return ValidationResult::Truncated;
        }

        // позиция внутри data
        let mut pos: usize = 0;

        // Читаем операцию
        let op = data[pos];
        pos = match pos.checked_add(1) {
            Some(p) => p,
            None => return ValidationResult::Truncated,
        };

        // Проверяем тип операции
        if op != 1 && op != 2 {
            // Set = 1, Del = 2
            return ValidationResult::UnknownOperation(op);
        }

        // Читаем checksum (4 байта) — убедимся, что есть 4 байта после pos
        let checksum_start = pos;
        if checksum_start
            .checked_add(4)
            .is_none_or(|end| end > data.len())
        {
            return ValidationResult::Truncated;
        }

        let expected_checksum = u32::from_be_bytes([
            data[checksum_start],
            data[checksum_start + 1],
            data[checksum_start + 2],
            data[checksum_start + 3],
        ]);

        // продвигаем pos после checksum
        pos = checksum_start + 4;

        // Остальная часть записи (для checksum вычисления) — payload начинается после
        // checksum
        let payload = &data[pos..];

        // Проверяем что есть достаточно данных для key_len
        if payload.len() < 4 {
            return ValidationResult::Truncated;
        }

        // Читаем key_len
        let key_len = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;

        // check 4 + key_len doesn't overflow and payload has enough bytes
        let after_key = match 4usize.checked_add(key_len) {
            Some(v) => v,
            None => return ValidationResult::Truncated,
        };
        if payload.len() < after_key {
            return ValidationResult::Truncated;
        }

        // Для SET операции проверяем val_len
        if op == 1 {
            // need at least 4 more bytes for val_len after key
            let need = match after_key.checked_add(4) {
                Some(v) => v,
                None => return ValidationResult::Truncated,
            };
            if payload.len() < need {
                return ValidationResult::Truncated;
            }

            let val_len_pos = after_key;
            let val_len = u32::from_be_bytes([
                payload[val_len_pos],
                payload[val_len_pos + 1],
                payload[val_len_pos + 2],
                payload[val_len_pos + 3],
            ]) as usize;

            // compute total size and check overflow
            let total = match val_len_pos
                .checked_add(4)
                .and_then(|p| p.checked_add(val_len))
            {
                Some(t) => t,
                None => return ValidationResult::Truncated,
            };

            if payload.len() < total {
                return ValidationResult::Truncated;
            }
        }

        // Вычисляем checksum для payload
        let actual_checksum = self.crc32.checksum(payload);

        if actual_checksum != expected_checksum {
            ValidationResult::Corrupted {
                expected: expected_checksum,
                actual: actual_checksum,
            }
        } else {
            ValidationResult::Valid
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для Crc32, AofValidator
////////////////////////////////////////////////////////////////////////////////

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for AofValidator {
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет, что CRC32 возвращает известные значения для стандартных
    /// тестовых векторов
    #[test]
    fn test_crc32_known_values() {
        let crc = Crc32::new();

        // Тестовые векторы для CRC32
        assert_eq!(crc.checksum(b""), 0);
        assert_eq!(crc.checksum(b"a"), 0xe8b7be43);
        assert_eq!(crc.checksum(b"abc"), 0x352441c2);
        assert_eq!(crc.checksum(b"message digest"), 0x20159d7f);
    }

    /// Тест проверяет, что метод verify корректно подтверждает правильный
    /// checksum и отвергает неверный
    #[test]
    fn test_crc32_verify() {
        let crc = Crc32::new();
        let data = b"test data";
        let expected = crc.checksum(data);

        assert!(crc.verify(data, expected));
        assert!(!crc.verify(data, expected + 2))
    }

    /// Тест проверяет, что валидная SET запись проходит проверку валидности
    #[test]
    fn test_validator_valid_set_record() {
        let mut validator = AofValidator::new();

        // Создаём валидную SET запись
        let key = b"test_key";
        let value = b"test_value";

        // Payload: key_len + key + val_len + value
        let mut payload = Vec::new();
        payload.extend_from_slice(&(key.len() as u32).to_be_bytes());
        payload.extend_from_slice(key);
        payload.extend_from_slice(&(value.len() as u32).to_be_bytes());
        payload.extend_from_slice(value);

        let checksum = validator.compute_checksum(&payload);

        // Полная запись: op + checksum + payload
        let mut record = Vec::new();
        record.push(1); // SET operation
        record.extend_from_slice(&checksum.to_be_bytes());
        record.extend_from_slice(&payload);

        let result = validator.validate_record(&record);
        assert_eq!(result, ValidationResult::Valid);
        assert_eq!(validator.stats().records_valid, 1);
    }

    /// Тест проверяет, что валидная DEL запись проходит проверку валидности
    #[test]
    fn test_validator_valid_del_record() {
        let mut validator = AofValidator::new();
        let key = b"abc";
        let mut payload = Vec::new();
        payload.extend_from_slice(&(key.len() as u32).to_be_bytes());
        payload.extend_from_slice(key);

        let checksum = validator.compute_checksum(&payload);

        let mut record = Vec::new();
        record.push(2); // DEL
        record.extend_from_slice(&checksum.to_be_bytes());
        record.extend_from_slice(&payload);

        let result = validator.validate_record(&record);
        assert_eq!(result, ValidationResult::Valid);
        assert_eq!(validator.stats().records_valid, 1);
    }

    /// Тест проверяет, что запись с неверным checksum помечается как Corrupted
    #[test]
    fn test_validator_corrupted_checksum() {
        let mut validator = AofValidator::new();

        let key = b"test";
        let value = b"value";

        let mut payload = Vec::new();
        payload.extend_from_slice(&(key.len() as u32).to_be_bytes());
        payload.extend_from_slice(key);
        payload.extend_from_slice(&(value.len() as u32).to_be_bytes());
        payload.extend_from_slice(value);

        let wrong_checksum = 0xDEADBEEF_u32; // Неправильный checksum

        let mut record = Vec::new();
        record.push(1); // SET operation
        record.extend_from_slice(&wrong_checksum.to_be_bytes());
        record.extend_from_slice(&payload);

        let result = validator.validate_record(&record);
        assert!(matches!(result, ValidationResult::Corrupted { .. }));
        assert_eq!(validator.stats().records_corrupted, 1);
    }

    /// Тест проверяет, что усечённая запись помечается как Truncated
    #[test]
    fn test_validator_truncated_record() {
        let mut validator = AofValidator::new();

        // Слишком короткая запись
        let record = vec![1, 0, 0, 0]; // op + частичный checksum

        let result = validator.validate_record(&record);
        assert_eq!(result, ValidationResult::Truncated);
        assert_eq!(validator.stats().records_truncated, 1);
    }

    /// Тест проверяет, что запись с неизвестной операцией помечается как
    /// UnknownOperation
    #[test]
    fn test_validator_unknown_operation() {
        let mut validator = AofValidator::new();

        // Запись с неизвестной операцией
        let mut record = vec![99]; // Неизвестная операция
        record.extend_from_slice(&[0, 0, 0, 0]); // checksum
        record.extend_from_slice(&[0, 0, 0, 4]); // key_len
        record.extend_from_slice(b"test"); // key

        let result = validator.validate_record(&record);
        assert_eq!(result, ValidationResult::UnknownOperation(99));
        assert_eq!(validator.stats().records_unknown_op, 1);
    }

    /// Тест проверяет, что статистика целостности корректно обновляется при
    /// разных результатах валидации
    #[test]
    fn test_integrity_stats() {
        let mut stats = IntegrityStats::default();

        stats.update(&ValidationResult::Valid);
        stats.update(&ValidationResult::Valid);
        stats.update(&ValidationResult::Corrupted {
            expected: 1,
            actual: 2,
        });

        assert_eq!(stats.records_checked, 3);
        assert_eq!(stats.records_valid, 2);
        assert_eq!(stats.records_corrupted, 1);

        // сравнение с допуском
        let rate = stats.success_rate();
        assert!((rate - 66.66666666666667).abs() < 1e-9, "rate = {}", rate);

        // corruption 33% > 5% -> критическая проблема
        assert!(stats.has_critical_issues());
    }

    /// Тест проверяет, что критические проблемы с целостностью обнаруживаются
    /// корректно
    #[test]
    fn test_critical_issues_detection() {
        let mut stats = IntegrityStats::default();

        // 10% corruption - критично
        for _ in 0..9 {
            stats.update(&ValidationResult::Valid);
        }
        stats.update(&ValidationResult::Corrupted {
            expected: 1,
            actual: 2,
        });

        assert!(stats.has_critical_issues());
    }
}
