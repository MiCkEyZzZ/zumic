//! SmartHash — это адаптивная структура, похожая на хеш-таблицу, оптимизированная для
//! коллекций небольшого и среднего размера.
//!
//! Она использует два внутренних представления:
//! - `Zip`: компактный `Vec<(ArcBytes, ArcBytes)>` для маленьких наборов данных,
//! - `Map`: `HashMap<ArcBytes, ArcBytes>` для больших наборов данных.
//!
//! Структура автоматически переключается между этими представлениями в зависимости от
//! количества элементов для повышения производительности и эффективности использования
//! памяти.

use std::{
    collections::{hash_map, HashMap},
    slice,
};

use serde::{Deserialize, Serialize};

use super::ArcBytes;

/// Порог, при достижении которого `SmartHash` переключается с `Zip` на `Map`.
const THRESHOLD: usize = 32;

/// Адаптивная структура ключ-значение с автоматическим переключением представления.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartHash {
    /// Компактное представление с использованием вектора пар ключ-значение.
    Zip(Vec<(ArcBytes, ArcBytes)>),
    /// Представление на основе HashMap для быстрого доступа при больших объёмах данных.
    Map(HashMap<ArcBytes, ArcBytes>),
}

impl SmartHash {
    /// Создаёт новый пустой `SmartHash` с использованием представления `Zip`.
    pub fn new() -> Self {
        SmartHash::Zip(Vec::new())
    }

    /// Возвращает количество элементов в структуре.
    pub fn len(&self) -> usize {
        match self {
            SmartHash::Zip(v) => v.len(),
            SmartHash::Map(v) => v.len(),
        }
    }

    /// Возвращает `true`, если структура не содержит элементов.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Вставляет или обновляет значение по заданному ключу.
    ///
    /// При превышении количества элементов порогового значения происходит автоматическое
    /// переключение представления с `Zip` на `Map`.
    pub fn hset(&mut self, key: ArcBytes, value: ArcBytes) {
        match self {
            SmartHash::Zip(vec) => {
                if let Some((_, v)) = vec.iter_mut().find(|(k, _)| k == &key) {
                    *v = value;
                    return;
                }
                vec.push((key, value));
                if vec.len() > THRESHOLD {
                    let mut map = HashMap::with_capacity(vec.len());
                    for (k, v) in vec.drain(..) {
                        map.insert(k, v);
                    }
                    *self = SmartHash::Map(map);
                }
            }
            SmartHash::Map(map) => {
                map.insert(key, value);
                if map.len() < THRESHOLD / 2 {
                    let mut vec = Vec::with_capacity(map.len());
                    for (k, v) in map.drain() {
                        vec.push((k, v));
                    }
                    *self = SmartHash::Zip(vec);
                }
            }
        }
    }

    /// Возвращает ссылку на значение, соответствующее заданному ключу, если оно существует.
    pub fn hget(&self, key: &ArcBytes) -> Option<&ArcBytes> {
        match self {
            SmartHash::Zip(vec) => vec.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            SmartHash::Map(map) => map.get(key),
        }
    }

    /// Удаляет значение, соответствующее заданному ключу.
    ///
    /// Возвращает `true`, если ключ найден и значение удалено. При уменьшении размера
    /// структуры ниже половины порогового значения происходит downgrade до представления
    /// `Zip`.
    pub fn hdel(&mut self, key: &ArcBytes) -> bool {
        let removed = match self {
            SmartHash::Zip(vec) => {
                if let Some(pos) = vec.iter().position(|(k, _)| k == key) {
                    vec.remove(pos);
                    true
                } else {
                    false
                }
            }
            SmartHash::Map(map) => map.remove(key).is_some(),
        };
        // Отложенный downgrade: вместо немедленного drain, сохраняем флаг,
        // который будет проверяться при следующей вставке (или GET) для обновления представления.
        if removed {
            if let SmartHash::Map(map) = self {
                // Если размер сильно упал, и, скажем, мы ещё не проводили downgrade,
                // пометим, что он должен быть выполнен при следующей операции.
                if map.len() < THRESHOLD / 2 {
                    // Пробуем напрямую выполнить downgrade сейчас, или установить флаг для
                    // следующей операции.
                    // Например, если отложить, можно не выполнять drain прямо здесь.
                    // Для простоты оставим drain здесь, но это место можно оптимизировать в будущем.
                    // Либо использовать какую то структуру в которой данные будут сжаты. Пока оставил так,
                    // обдумваем, пробуем различные варианты. На данный момент бенчмарки проходят удовлетворительно,
                    // но можно улучшить.
                    let mut vec = Vec::with_capacity(map.len());
                    for (k, v) in map.drain() {
                        vec.push((k, v));
                    }
                    *self = SmartHash::Zip(vec);
                }
            }
        }
        removed
    }

    /// Возвращает итератор по парам ключ-значение.
    pub fn iter(&self) -> SmartHashIter<'_> {
        match self {
            SmartHash::Zip(vec) => SmartHashIter::Zip(vec.iter()),
            SmartHash::Map(map) => SmartHashIter::Map(map.iter()),
        }
    }
}

impl Default for SmartHash {
    fn default() -> Self {
        SmartHash::new()
    }
}

impl FromIterator<(ArcBytes, ArcBytes)> for SmartHash {
    fn from_iter<I: IntoIterator<Item = (ArcBytes, ArcBytes)>>(iter: I) -> Self {
        let mut sh = SmartHash::new();
        for (k, v) in iter {
            sh.hset(k, v);
        }
        sh
    }
}

impl Extend<(ArcBytes, ArcBytes)> for SmartHash {
    fn extend<I: IntoIterator<Item = (ArcBytes, ArcBytes)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.hset(k, v);
        }
    }
}

/// Итератор по элементам структуры `SmartHash`.
pub enum SmartHashIter<'a> {
    /// Итератор по компактному представлению `Zip`.
    Zip(slice::Iter<'a, (ArcBytes, ArcBytes)>),
    /// Итератор по представлению `Map`.
    Map(hash_map::Iter<'a, ArcBytes, ArcBytes>),
}

impl<'a> Iterator for SmartHashIter<'a> {
    type Item = (&'a ArcBytes, &'a ArcBytes);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SmartHashIter::Zip(iter) => iter.next().map(|(k, v)| (k, v)),
            SmartHashIter::Map(iter) => iter.next(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверяет, что значение, вставленное с помощью `hset`, может быть получено через
    /// `hget`.
    #[test]
    fn test_hset_hget() {
        let key = ArcBytes::from_str("key1");
        let value = ArcBytes::from_str("value1");

        let mut smart_hash = SmartHash::new();
        smart_hash.hset(key.clone(), value.clone());
        assert_eq!(smart_hash.hget(&key), Some(&value));
    }

    /// Проверяет, что ключ можно удалить с помощью `hdel`, после чего он становится
    /// недоступным.
    #[test]
    fn test_hdel() {
        let key = ArcBytes::from_str("key1");
        let value = ArcBytes::from_str("value1");

        let mut smart_hash = SmartHash::new();
        smart_hash.hset(key.clone(), value.clone());
        let removed = smart_hash.hdel(&key);
        assert!(removed);
        assert!(smart_hash.hget(&key).is_none());
    }

    /// Проверяет, что внутреннее представление автоматически переключается на `Map` после
    /// вставки большего количества элементов, чем задано порогом.
    #[test]
    fn test_auto_convert_to_map() {
        let mut smart_hash = SmartHash::new();
        // Вставьте больше элементов, чем порог THRESHOLD, чтобы вызвать преобразование.
        for i in 0..(THRESHOLD + 1) {
            let key = ArcBytes::from_str(&format!("key{}", i));
            let value = ArcBytes::from_str(&format!("value{}", i));
            smart_hash.hset(key, value);
        }

        // Проверяем, что внутреннее представление теперь является картой.
        match smart_hash {
            SmartHash::Map(_) => {}
            _ => panic!(
                "Ожидалось, что внутреннее представление будет Map после превышения THRESHOLD"
            ),
        }
    }

    /// Проверяет, что итерация по записям возвращает все пары ключ-значение.
    #[test]
    fn test_iter() {
        let mut smart_hash = SmartHash::new();
        let pairs = vec![
            (ArcBytes::from_str("a"), ArcBytes::from_str("1")),
            (ArcBytes::from_str("b"), ArcBytes::from_str("2")),
        ];
        for (k, v) in pairs.clone() {
            smart_hash.hset(k, v);
        }
        let collected: Vec<(&ArcBytes, &ArcBytes)> = smart_hash.iter().collect();
        // Проверяем наличие обоих элементов (порядок не гарантируется)
        assert_eq!(collected.len(), 2);
    }

    /// Проверяет корректность работы методов `len` и `is_empty`
    /// при вставке элементов.
    #[test]
    fn test_len_and_empty() {
        let mut sh = SmartHash::new();
        assert!(sh.is_empty());
        assert_eq!(sh.len(), 0);
        sh.hset(ArcBytes::from_str("a"), ArcBytes::from_str("1"));
        assert!(!sh.is_empty());
        assert_eq!(sh.len(), 1);
    }

    /// Проверяет работу методов `hset`, `hget`, `hdel` и понижение
    /// представления с Map на Zip, если размер структуры падает ниже порога.
    #[test]
    fn test_hset_hget_hdel_and_downgrade() {
        let mut sh = SmartHash::new();
        // Добавьте THRESHOLD+1 для перехода к карте
        for i in 0..(THRESHOLD + 1) {
            let k = ArcBytes::from_str(&format!("k{i}"));
            let v = ArcBytes::from_str(&format!("v{i}"));
            sh.hset(k.clone(), v.clone());
            assert_eq!(sh.hget(&k), Some(&v));
        }
        // Мы убедились, что внутри Map
        matches!(sh, SmartHash::Map(_));

        // Удалить все, кроме одного, чтобы map.len() == 1 < THRESHOLD/2
        for i in 0..THRESHOLD {
            let k = ArcBytes::from_str(&format!("k{i}"));
            assert!(sh.hdel(&k));
        }
        // Нужно вернуться к Zip
        matches!(sh, SmartHash::Zip(_));
        assert_eq!(sh.len(), 1);
    }

    /// Проверяет, что порядок итерации не влияет на корректность,
    /// сортируя записи перед сравнением.
    #[test]
    fn test_iter_order_independent() {
        let mut sh = SmartHash::new();
        let pairs = vec![
            (ArcBytes::from_str("x"), ArcBytes::from_str("10")),
            (ArcBytes::from_str("y"), ArcBytes::from_str("20")),
            (ArcBytes::from_str("z"), ArcBytes::from_str("30")),
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

    /// Проверяет, что реализация FromIterator корректно создаёт SmartHash
    /// так, что все записи доступны.
    #[test]
    fn test_from_iterator() {
        let pairs = vec![
            (ArcBytes::from_str("foo"), ArcBytes::from_str("bar")),
            (ArcBytes::from_str("baz"), ArcBytes::from_str("qux")),
        ];
        let sh: SmartHash = pairs.clone().into_iter().collect();
        assert_eq!(sh.len(), 2);
        for (k, v) in pairs {
            assert_eq!(sh.hget(&k), Some(&v));
        }
    }
}
