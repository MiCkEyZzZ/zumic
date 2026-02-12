//! - `Zip`: компактный `Vec<(ArcBytes, ArcBytes)>` для маленьких наборов
//!   данных.
//!
//! - `Map`: `HashMap<ArcBytes, ArcBytes>` для больших наборов данных.
//!
//! Структура автоматически переключается между этими представлениями в
//! зависимости от количества элементов для повышения производительности и
//! эффективности использования памяти.

use std::{
    collections::{hash_map, HashMap},
    slice,
};

use serde::{Deserialize, Serialize};

use crate::Sds;

/// Порог, при достижении которого `SmartHash` переключается с
/// `Zip` на `Map`.
const THRESHOLD: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Repr {
    /// Компактное представление с использованием вектора пар
    /// ключ-значение.
    Zip(Vec<(Sds, Sds)>),

    /// Хеш‑таблица для больших объёмов.
    Map(HashMap<Sds, Sds>),
}

/// Адаптивная структура «хеш‑таблица» с автоматическим
/// переключением внутреннего представления.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartHash {
    repr: Repr,

    #[serde(skip)]
    pending_downgrade: bool,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl SmartHash {
    /// Создаёт пустой SmartHash (начинаем в Zip‑режиме).
    pub fn new() -> Self {
        SmartHash {
            repr: Repr::Zip(Vec::new()),
            pending_downgrade: false,
        }
    }

    /// Возвращает количество элементов в структуре.
    pub fn len(&self) -> usize {
        match &self.repr {
            Repr::Zip(v) => v.len(),
            Repr::Map(v) => v.len(),
        }
    }

    /// Возвращает `true`, если структура не содержит элементов.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Проверяет наличие ключа.
    pub fn contains(
        &self,
        key: &Sds,
    ) -> bool {
        match &self.repr {
            Repr::Zip(v) => v.iter().any(|(k, _)| k == key),
            Repr::Map(m) => m.contains_key(key),
        }
    }

    /// Вставляем или обновляем пару (key, value).
    ///
    /// При превышении количества элементов порогового значения
    /// происходит автоматическое переключение представления с
    /// `Zip` на `Map`.
    pub fn insert(
        &mut self,
        key: Sds,
        value: Sds,
    ) -> bool {
        if self.pending_downgrade {
            self.do_downgrade();
        }

        match &mut self.repr {
            Repr::Zip(vec) => {
                if let Some((_, v)) = vec.iter_mut().find(|(k, _)| *k == key) {
                    *v = value;
                    return false;
                }

                vec.push((key, value));

                if vec.len() >= THRESHOLD {
                    let mut map = HashMap::with_capacity(vec.len());
                    for (k, v) in vec.drain(..) {
                        map.insert(k, v);
                    }
                    self.repr = Repr::Map(map);
                }

                true
            }
            Repr::Map(map) => {
                map.insert(key, value).is_none()
                // HashMap::insert возвращает:
                // Some(old_value) если ключ существовал
                // None если ключ новый
            }
        }
    }

    /// Возвращает ссылку на значение, соответствующее заданному
    /// ключу, если оно существует.
    pub fn get(
        &self,
        key: &Sds,
    ) -> Option<&Sds> {
        match &self.repr {
            Repr::Zip(vec) => vec.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            Repr::Map(map) => map.get(key),
        }
    }

    /// Удаляет значение, соответствующее заданному ключу.
    ///
    /// Возвращает `true`, если ключ найден и значение удалено.
    /// При уменьшении размера структуры ниже половины порогового
    /// значения происходит downgrade до представления `Zip`.
    pub fn remove(
        &mut self,
        key: &Sds,
    ) -> bool {
        let removed = match &mut self.repr {
            Repr::Zip(vec) => {
                if let Some(pos) = vec.iter().position(|(k, _)| k == key) {
                    vec.remove(pos);
                    true
                } else {
                    false
                }
            }
            Repr::Map(map) => map.remove(key).is_some(),
        };
        // Отложенный downgrade: вместо немедленного drain, сохраняем флаг,
        // который будет проверяться при следующей вставке (или GET) для обновления
        // представления.
        if removed {
            if let Repr::Map(map) = &self.repr {
                // Если размер сильно упал, и, скажем, мы ещё не проводили downgrade,
                // пометим, что он должен быть выполнен при следующей операции.
                if map.len() < THRESHOLD / 2 {
                    self.pending_downgrade = true;
                }
            }
        }
        removed
    }

    /// HGETALL: в виде Vec<(String, String)>
    pub fn get_all(&self) -> Vec<(String, String)> {
        self.entries()
            .into_iter()
            .map(|(k, v)| {
                (
                    String::from_utf8_lossy(k.as_slice()).into_owned(),
                    String::from_utf8_lossy(v.as_slice()).into_owned(),
                )
            })
            .collect()
    }

    /// Проводит реальный даунгрейд из Map в Zip (вызывается лениво).
    fn do_downgrade(&mut self) {
        if let Repr::Map(mut map) = std::mem::replace(&mut self.repr, Repr::Zip(Vec::new())) {
            let mut vec = Vec::with_capacity(map.len());
            for (k, v) in map.drain() {
                vec.push((k, v));
            }
            self.repr = Repr::Zip(vec);
        }
        self.pending_downgrade = false;
    }

    /// Очищает все записи.
    pub fn clear(&mut self) {
        self.repr = Repr::Zip(Vec::new());
        self.pending_downgrade = false;
    }

    /// Список всех ключей (ненумерованный порядок).
    pub fn keys(&self) -> Vec<Sds> {
        match &self.repr {
            Repr::Zip(v) => v.iter().map(|(k, _)| k.clone()).collect(),
            Repr::Map(m) => m.keys().cloned().collect(),
        }
    }

    /// Список всех значений.
    pub fn values(&self) -> Vec<Sds> {
        match &self.repr {
            Repr::Zip(v) => v.iter().map(|(_, v)| v.clone()).collect(),
            Repr::Map(m) => m.values().cloned().collect(),
        }
    }

    /// Список всех пар (key, value).
    pub fn entries(&self) -> Vec<(Sds, Sds)> {
        match &self.repr {
            Repr::Zip(v) => v.clone(),
            Repr::Map(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        }
    }

    /// Возвращает итератор по парам ключ-значение.
    pub fn iter(&mut self) -> SmartHashIter<'_> {
        if self.pending_downgrade {
            self.do_downgrade();
        }

        match &self.repr {
            Repr::Zip(vec) => SmartHashIter::Zip(vec.iter()),
            Repr::Map(map) => SmartHashIter::Map(map.iter()),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для SmartHash
////////////////////////////////////////////////////////////////////////////////

impl Default for SmartHash {
    fn default() -> Self {
        SmartHash::new()
    }
}

impl FromIterator<(Sds, Sds)> for SmartHash {
    fn from_iter<I: IntoIterator<Item = (Sds, Sds)>>(iter: I) -> Self {
        let mut sh = SmartHash::new();
        for (k, v) in iter {
            sh.insert(k, v);
        }
        sh
    }
}

impl Extend<(Sds, Sds)> for SmartHash {
    fn extend<I: IntoIterator<Item = (Sds, Sds)>>(
        &mut self,
        iter: I,
    ) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

/// Итератор по элементам структуры `SmartHash`.
pub enum SmartHashIter<'a> {
    /// Итератор по компактному представлению `Zip`.
    Zip(slice::Iter<'a, (Sds, Sds)>),

    /// Итератор по представлению `Map`.
    Map(hash_map::Iter<'a, Sds, Sds>),
}

impl<'a> Iterator for SmartHashIter<'a> {
    type Item = (&'a Sds, &'a Sds);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SmartHashIter::Zip(iter) => iter.next().map(|(k, v)| (k, v)),
            SmartHashIter::Map(iter) => iter.next(),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hset_hget() {
        let mut smart_hash = SmartHash::new();
        let key = Sds::from_str("key1");
        let value = Sds::from_str("value1");

        smart_hash.insert(key.clone(), value.clone());
        assert_eq!(smart_hash.get(&key), Some(&value));
    }

    #[test]
    fn test_hdel_and_empty() {
        let mut sh = SmartHash::new();
        let k = Sds::from_str("kin");
        let v = Sds::from_str("za");
        sh.insert(k.clone(), v);
        assert!(sh.remove(&k));
        assert!(!sh.contains(&k));
        assert!(sh.is_empty());
    }

    #[test]
    fn test_auto_convert_to_map_and_lazy_downgrade() {
        let mut sh = SmartHash::new();
        // переполним Zip → Map
        for i in 0..(THRESHOLD + 1) {
            let k = Sds::from_str(&i.to_string());
            sh.insert(k, Sds::from_str("v"));
        }
        assert!(matches!(sh.repr, Repr::Map(_)));

        // удалим всё
        for i in 0..(THRESHOLD + 1) {
            let k = Sds::from_str(&i.to_string());
            assert!(sh.remove(&k));
        }
        // ещё не downgraded
        assert!(sh.pending_downgrade);

        // на первой же hset произойдёт downgrade
        sh.insert(Sds::from_str("x"), Sds::from_str("y"));
        assert!(!sh.pending_downgrade);
        assert!(matches!(sh.repr, Repr::Zip(_)));
    }

    #[test]
    fn test_iter_and_entries_order_independent() {
        let mut sh = SmartHash::new();
        let pairs = vec![
            (Sds::from_str("a"), Sds::from_str("1")),
            (Sds::from_str("b"), Sds::from_str("2")),
            (Sds::from_str("c"), Sds::from_str("3")),
        ];
        sh.extend(pairs.clone());
        let mut got: Vec<_> = sh.iter().collect();
        let mut expected: Vec<_> = pairs.iter().map(|(k, v)| (k, v)).collect();
        got.sort_by(|(a, _), (b, _)| a.cmp(b));
        expected.sort_by(|(a, _), (b, _)| a.cmp(b));
        assert_eq!(got, expected);
    }

    #[test]
    fn test_len_and_clear() {
        let mut sh = SmartHash::new();
        assert_eq!(sh.len(), 0);
        sh.insert(Sds::from_str("a"), Sds::from_str("1"));
        assert_eq!(sh.len(), 1);
        sh.clear();
        assert_eq!(sh.len(), 0);
    }

    #[test]
    fn test_keys_values_entries_hget_all() {
        let mut sh = SmartHash::new();
        sh.insert(Sds::from_str("x"), Sds::from_str("10"));
        sh.insert(Sds::from_str("y"), Sds::from_str("20"));
        let keys = sh.keys();
        assert!(keys.contains(&Sds::from_str("x")));
        let values = sh.values();
        assert!(values.contains(&Sds::from_str("20")));
        let entries = sh.entries();
        assert!(entries.iter().any(|(k, _)| k == &Sds::from_str("y")));
        let frame = sh.get_all();
        assert!(frame.iter().any(|(k, v)| k == "x" && v == "10"));
    }

    #[test]
    fn test_iter_order_independent() {
        let mut sh = SmartHash::new();
        let pairs = vec![
            (Sds::from_str("x"), Sds::from_str("10")),
            (Sds::from_str("y"), Sds::from_str("20")),
            (Sds::from_str("z"), Sds::from_str("30")),
        ];
        // Использование расширения
        sh.extend(pairs.clone());
        let mut got: Vec<_> = sh.iter().collect();
        // Сортировать по ключу для стабильности
        got.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut expected = pairs.clone();
        expected.sort_by(|(a, _), (b, _)| a.cmp(b));
        let expected_refs: Vec<_> = expected.iter().map(|(k, v)| (k, v)).collect();
        assert_eq!(got, expected_refs);
    }

    #[test]
    fn test_from_iterator() {
        let pairs = vec![
            (Sds::from_str("k1"), Sds::from_str("v1")),
            (Sds::from_str("k2"), Sds::from_str("v2")),
        ];
        let sh: SmartHash = pairs.clone().into_iter().collect();
        assert_eq!(sh.len(), 2);
        for (k, v) in pairs {
            assert_eq!(sh.get(&k), Some(&v));
        }
    }
}
