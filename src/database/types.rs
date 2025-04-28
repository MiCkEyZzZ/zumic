use std::collections::{HashMap, HashSet};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use super::{QuickList, Sds, SkipList, SmartHash};

/// Представляет обобщённое значение в движке хранения данных.
///
/// Используется как основной контейнер для различных поддерживаемых типов
/// данных: строк, целых чисел, чисел с плавающей точкой, `null`, коллекций
/// (списки, множества, хэши, упорядоченные множества), а также более сложных
/// структур, таких как HyperLogLog и потоки.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Value {
    /// Двоично-безопасная строка.
    Str(Sds),
    /// Знаковое 64-битное целое число.
    Int(i64),
    /// 64-битное число с плавающей точкой.
    Float(f64),
    /// Тип `null` (используется как маркер отсутствия значения или удаления).
    Null,
    /// Список двоичных строк, реализованный через `QuickList`.
    List(QuickList<Sds>),
    /// Хэш-карта (словарь), хранимая как `SmartHash`.
    Hash(SmartHash),
    /// Упорядоченное множество с сортировкой по оценке (`score`).
    ///
    /// Поле `dict` сопоставляет каждый элемент со значением оценки,
    /// а `sorted` поддерживает упорядоченный список ключей по оценкам.
    ZSet {
        /// Соответствие элемента его оценке.
        dict: HashMap<Sds, f64>,
        /// Список элементов, упорядоченных по оценкам.
        sorted: SkipList<OrderedFloat<f64>, Sds>,
    },
    /// Множество уникальных строковых элементов.
    Set(HashSet<Sds>),
    /// Структура HyperLogLog для приближённого подсчёта количества уникальных элементов.
    HyperLogLog(HLL),
    /// Поток записей, каждая запись имеет идентификатор и набор полей.
    SStream(Vec<StreamEntry>),
}

/// Запись потока данных.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamEntry {
    /// Уникальный идентификатор записи в потоке.
    pub id: u64,
    /// Поля записи и их значения.
    pub data: HashMap<String, Value>,
}

/// Структура HyperLogLog для приближённого уникального счётчика.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HLL {
    /// Внутренние регистры, используемые алгоритмом HyperLogLog.
    pub registers: Vec<u8>,
}
