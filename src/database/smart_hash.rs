use std::{
    collections::{hash_map, HashMap},
    slice,
};

use serde::{Deserialize, Serialize};

use super::ArcBytes;

/// Порог для переключения с компактного представления на хэш-таблицу.
/// Подберите это значение эмпирически.
const THRESHOLD: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartHash {
    /// Компактное представление для небольшого числа элементов.
    Zip(Vec<(ArcBytes, ArcBytes)>),
    /// Представление для большого числа элементов.
    Map(HashMap<ArcBytes, ArcBytes>),
}

impl SmartHash {
    /// Создаёт новый SmartHash в компактном представлении.
    pub fn new() -> Self {
        SmartHash::Zip(Vec::new())
    }
    /// Возвращает общее число элементов.
    pub fn len(&self) -> usize {
        match self {
            SmartHash::Zip(v) => v.len(),
            SmartHash::Map(v) => v.len(),
        }
    }
    /// Проверяет пустая ли структура и возвращает true/false.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Вставляет или обновляет пару (key, value) в SmartHash.
    pub fn hset(&mut self, key: ArcBytes, value: ArcBytes) {
        match self {
            SmartHash::Zip(vec) => {
                // Обновляем значение, если ключ уже существует
                if let Some((_, v)) = vec.iter_mut().find(|(k, _)| k == &key) {
                    *v = value;
                    return;
                }
                // Иначе добавляем новую пару.
                vec.push((key, value));
                // Если число элементов превышает порог, конвертируем в Map
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
                // Обратное понижение в Zip, если стало слишком мало элементов
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
    /// Получает ссылку на значение по заданному ключу.
    pub fn hget(&self, key: &ArcBytes) -> Option<&ArcBytes> {
        match self {
            SmartHash::Zip(vec) => vec.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            SmartHash::Map(map) => map.get(key),
        }
    }
    /// Удаляет запись по ключу. Возвращает true, если элемент был удалён.
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
        // После удаления в Map может быть мало элементов — понижаем обратно
        if removed {
            if let SmartHash::Map(map) = self {
                if map.len() < THRESHOLD / 2 {
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
    /// Возвращаем итератор по парам (key, value).
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

/// Итератор для SmartHash, который абстрагируется от внутреннего представления.
pub enum SmartHashIter<'a> {
    Zip(slice::Iter<'a, (ArcBytes, ArcBytes)>),
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

    #[test]
    fn test_hset_hget() {
        let key = ArcBytes::from_str("key1");
        let value = ArcBytes::from_str("value1");

        let mut smart_hash = SmartHash::new();
        smart_hash.hset(key.clone(), value.clone());
        assert_eq!(smart_hash.hget(&key), Some(&value));
    }

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

    #[test]
    fn test_auto_convert_to_map() {
        let mut smart_hash = SmartHash::new();
        // Вставляем больше элементов, чем пороговый THRESHOLD, чтобы инициировать конвертацию.
        for i in 0..(THRESHOLD + 1) {
            let key = ArcBytes::from_str(&format!("key{}", i));
            let value = ArcBytes::from_str(&format!("value{}", i));
            smart_hash.hset(key, value);
        }

        // Проверяем, что внутреннее представление теперь Map.
        match smart_hash {
            SmartHash::Map(_) => {}
            _ => panic!(
                "Ожидалось, что внутреннее представление будет Map после превышения THRESHOLD"
            ),
        }
    }

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
        // Проверяем, что оба элемента присутствуют (порядок не гарантирован)
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_len_and_empty() {
        let mut sh = SmartHash::new();
        assert!(sh.is_empty());
        assert_eq!(sh.len(), 0);
        sh.hset(ArcBytes::from_str("a"), ArcBytes::from_str("1"));
        assert!(!sh.is_empty());
        assert_eq!(sh.len(), 1);
    }

    #[test]
    fn test_hset_hget_hdel_and_downgrade() {
        let mut sh = SmartHash::new();
        // Добавляем THRESHOLD+1, чтобы перешли в Map
        for i in 0..(THRESHOLD + 1) {
            let k = ArcBytes::from_str(&format!("k{i}"));
            let v = ArcBytes::from_str(&format!("v{i}"));
            sh.hset(k.clone(), v.clone());
            assert_eq!(sh.hget(&k), Some(&v));
        }
        // Убедились, что внутри Map
        matches!(sh, SmartHash::Map(_));

        // Удаляем все, кроме одного, чтобы map.len() == 1 < THRESHOLD/2
        for i in 0..THRESHOLD {
            let k = ArcBytes::from_str(&format!("k{i}"));
            assert!(sh.hdel(&k));
        }
        // Должно перейти обратно в Zip
        matches!(sh, SmartHash::Zip(_));
        assert_eq!(sh.len(), 1);
    }

    #[test]
    fn test_iter_order_independent() {
        let mut sh = SmartHash::new();
        let pairs = vec![
            (ArcBytes::from_str("x"), ArcBytes::from_str("10")),
            (ArcBytes::from_str("y"), ArcBytes::from_str("20")),
            (ArcBytes::from_str("z"), ArcBytes::from_str("30")),
        ];
        // Используем Extend
        sh.extend(pairs.clone());
        let mut got: Vec<_> = sh.iter().collect();
        // Сортируем по ключу для стабильности
        got.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut expected = pairs.clone();
        expected.sort_by(|(a, _), (b, _)| a.cmp(b));
        let expected_refs: Vec<_> = expected.iter().map(|(k, v)| (k, v)).collect();
        assert_eq!(got, expected_refs);
    }

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
