use std::{collections::HashSet, io::Cursor};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use super::{Bitmap, Dict, Hll, QuickList, Sds, SkipList, SmartHash, StreamEntry};
use crate::{
    engine::{decode, encode},
    StoreError, StoreResult,
};

/// Представляет универсальное значение в движке хранения данных.
///
/// Это основной контейнер для различных поддерживаемых типов данных:
/// строк, целых и числовых значений с плавающей точкой, `null`,
/// коллекций (списки, множества, хеши, отсортированные множества),
/// а также более сложных структур — HyperLogLog и потоков.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Value {
    /// Безопасная бинарная строка.
    Str(Sds),
    /// Целое 64-битное число.
    Int(i64),
    /// Число с плавающей точкой двойной точности (64-бит).
    Float(f64),
    /// Булево значение.
    Bool(bool),
    /// Тип `null` — отсутствие значения или удаление.
    Null,
    /// Список бинарных строк, реализованный через `QuickList`.
    List(QuickList<Sds>),
    /// Массив произвольных значений RESP (Array в протоколе).
    /// Используется для ответов на команды, возвращающие массивы
    /// разнородных Value.
    Array(Vec<Value>),
    /// Хэш-таблица (словарь), реализованная через `SmartHash`.
    Hash(SmartHash),
    /// Отсортированное множество с упорядочиванием по скору.
    ///
    /// Поле `dict` сопоставляет элементу его скор,
    /// а `sorted` поддерживает порядок элементов по значению скора.
    ZSet {
        /// Отображение элементов в их скор.
        dict: Dict<Sds, f64>,
        /// Структура для поддержания сортировки по скору.
        sorted: SkipList<OrderedFloat<f64>, Sds>,
    },
    /// Множество уникальных строковых элементов.
    Set(HashSet<Sds>),
    /// Структура HyperLogLog для приблизительного подсчёта количества
    /// уникальных элементов.
    HyperLogLog(Box<Hll>),
    /// Поток записей, каждая из которых идентифицируется ID и набором полей.
    SStream(Vec<StreamEntry>),
    /// Битовый массив для команд SETBIT/GETBIT/BITCOUNT/BITOP
    Bitmap(Bitmap),
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl Value {
    /// Сериализует значение в бинарный формат через ZDB encode.
    ///
    /// Возвращает вектор байт с закодированным значением.
    ///
    /// # Паника
    ///
    /// Паника при ошибке сериализации, так как предполагается
    /// корректность данных.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode::write_value(&mut buf, self).expect("ZDB serialization failed");
        buf
    }

    /// Десериализует значение из бинарного формата через ZDB decode.
    ///
    /// Возвращает результат с десериализованным значением или ошибку.
    pub fn from_bytes(bytes: &[u8]) -> StoreResult<Value> {
        let mut cursor = Cursor::new(bytes);
        decode::read_value(&mut cursor).map_err(|e| StoreError::SerdeError(e.to_string()))
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&Sds> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }
}

// NOTE: ВРЕМЕННАЯ ЗАГЛУШКА. ПОСЛЕ РЕАЛИЗАЦИИ SEND + SYNC В SKIPLIST
unsafe impl Send for Value {}
unsafe impl Sync for Value {}
