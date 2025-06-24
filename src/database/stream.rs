use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::Value;

/// Уникальный идентификатор записи в потоке.
/// Состоит из времени в миллисекундах и порядкового номера (sequence).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StreamId {
    /// Время создания записи в миллисекундах с эпохи UNIX
    pub ms_time: u64,
    /// Порядковый номер записи в пределах одной миллисекунды
    pub sequence: u64,
}

/// Запись потока — содержит идентификатор и данные.
/// Данные — это ассоциативный массив (ключ-значение) с произвольными значениями.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamEntry {
    pub id: StreamId,
    pub data: HashMap<String, Value>,
}

/// Поток — структура, хранящая упорядоченный список записей.
/// Использует атомарный счётчик для генерации уникальных идентификаторов.
#[derive(Debug, Default)]
pub struct Stream {
    /// Очередь записей
    entries: VecDeque<StreamEntry>,
    /// Атомарный счетчик для sequence
    next_sequence: AtomicU64,
}

impl Stream {
    /// Создает новый пустой поток
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            next_sequence: AtomicU64::new(1),
        }
    }

    /// Добавляет новую запись с данными в поток.
    /// Автоматически создаёт уникальный идентификатор на основе текущего
    /// времени и sequence.
    /// Возвращает созданный StreamId.
    pub fn add(
        &mut self,
        data: HashMap<String, Value>,
    ) -> StreamId {
        let ms_time = Self::current_millis();
        // Получаем следующий sequence атомарно (с использованием Relaxed порядка)
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let id = StreamId { ms_time, sequence };
        self.entries.push_back(StreamEntry {
            id: id.clone(),
            data,
        });
        id
    }

    /// Возвращает срез записей в диапазоне идентификаторов [start, end].
    /// Идентификаторы сравниваются с помощью PartialOrd и Ord.
    pub fn range(
        &self,
        start: &StreamId,
        end: &StreamId,
    ) -> Vec<&StreamEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.id >= *start && entry.id <= *end)
            .collect()
    }

    /// Итератор по всем записям в потоке в порядке их добавления.
    pub fn iter(&self) -> impl Iterator<Item = &StreamEntry> {
        self.entries.iter()
    }

    /// Возвращает количество записей в потоке.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Проверяет, пуст ли поток.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Вспомогательная функция для получения текущего времени в
    /// миллисекундах с эпохи UNIX.
    fn current_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Вспомогательная функция для создания записи с одним ключом и значением Int.
    fn make_entry(
        key: &str,
        val: i64,
    ) -> HashMap<String, Value> {
        let mut hm = HashMap::new();
        hm.insert(key.to_string(), Value::Int(val));
        hm
    }

    /// Тест проверяет методы new, is_empty и len, который корректно создаёт новый
    /// поток пустым, и что методы `is_empty` и `len` корректно отражают состояние.
    #[test]
    fn test_new_is_empty() {
        let stream = Stream::new();
        assert!(stream.is_empty());
        assert_eq!(stream.len(), 0);
    }

    /// Тест проверяет метод add, который корректно добавляет записи и sequence
    /// увеличивается, а поток становится непустым, и корректно меняется длина.
    #[test]
    fn test_add_increments_and_len() {
        let mut stream = Stream::new();
        let data1 = make_entry("a", 1);
        let data2 = make_entry("b", 2);

        let id1 = stream.add(data1.clone());
        assert!(!stream.is_empty());
        assert_eq!(stream.len(), 1);

        let id2 = stream.add(data2.clone());
        assert_eq!(stream.len(), 2);

        // Проверяем, что время ms_time одинаковое (тест может сломаться, если
        // время изменится между вызовами)
        assert_eq!(id2.ms_time, id1.ms_time);
        // Проверяем, что sequence увеличился на 1
        assert_eq!(id2.sequence, id1.sequence + 1);
    }

    /// Тест проверяет метод iter, который корректно возвращает записи в
    /// порядке добавления, а данные в записях совпадают с теми, что мы добавляли.
    #[test]
    fn test_iter_yields_entries_in_order() {
        let mut stream = Stream::new();
        let data1 = make_entry("x", 10);
        let data2 = make_entry("y", 20);

        let id1 = stream.add(data1.clone());
        let id2 = stream.add(data2.clone());

        let entries: Vec<_> = stream.iter().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, id1);
        assert_eq!(entries[1].id, id2);
        assert_eq!(entries[0].data.get("x"), Some(&Value::Int(10)));
        assert_eq!(entries[1].data.get("y"), Some(&Value::Int(20)));
    }

    /// Тест проверяет метод range, который должен вернуть записи,
    /// находящиеся в заданном диапазоне идентификаторов.
    #[test]
    fn test_range_limits_entries() {
        let mut stream = Stream::new();
        let data1 = make_entry("kin", 100);
        let data2 = make_entry("dza", 200);
        let data3 = make_entry("kuu", 300);

        let id1 = stream.add(data1.clone());
        let id2 = stream.add(data2.clone());
        let id3 = stream.add(data3.clone());

        // Диапазон от id1 до id2 должен содержать первые две записи
        let slice = stream.range(&id1, &id2);
        assert_eq!(slice.len(), 2);
        assert_eq!(slice[0].id, id1);
        assert_eq!(slice[1].id, id2);

        // Диапазон от id2 до id3 должен содержать последние две записи
        let slice2 = stream.range(&id2, &id3);
        assert_eq!(slice2.len(), 2);
        assert_eq!(slice2[0].id, id2);
        assert_eq!(slice2[1].id, id3);

        // Диапазон от минимального до максимального id должен вернуть все записи
        let before = StreamId {
            ms_time: 0,
            sequence: 0,
        };
        let after = StreamId {
            ms_time: u64::MAX,
            sequence: u64::MAX,
        };
        let full = stream.range(&before, &after);
        assert_eq!(full.len(), 3);
    }
}
