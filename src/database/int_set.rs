//! `IntSet` — компактное множество 64-битных целыхчисел с
//! автоматическим выбором размера хранения.
//!
//! Хранит уникальные целые числа в отсортированном виде,
//! используя минимально необходимое количество байт:
//! `i16`, `i32` или `i64`, в зависимости от наибольшего
//! вставленного значения. При необходимости кодировка
//! автоматически расширяется (upcast), чтобы вместить новые
//! значения.

use std::convert::TryInto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Int16,
    Int32,
    Int64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntSet {
    encoding: Encoding,
    data: Vec<u8>, // Всегда отсортирован и без дубликатов.
}

impl Encoding {
    fn bytes(self) -> usize {
        match self {
            Encoding::Int16 => 2,
            Encoding::Int32 => 4,
            Encoding::Int64 => 8,
        }
    }

    fn for_value(x: i64) -> Encoding {
        if x >= i16::MIN as i64 && x <= i16::MAX as i64 {
            Encoding::Int16
        } else if x >= i32::MIN as i64 && x <= i32::MAX as i64 {
            Encoding::Int32
        } else {
            Encoding::Int64
        }
    }
}

impl IntSet {
    pub fn new() -> Self {
        IntSet {
            encoding: Encoding::Int16,
            data: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len() / self.encoding.bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Бинарный поиск, возвращает (found,pos). pos — место вставки, если не найдено
    fn find(&self, value: i64) -> (bool, usize) {
        let mut lo = 0;
        let mut hi = self.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            let mid_val = self.read_at(mid);
            if mid_val == value {
                return (true, mid);
            } else if mid_val < value {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        (false, lo)
    }

    pub fn contains(&self, value: i64) -> bool {
        self.find(value).0
    }

    pub fn insert(&mut self, value: i64) -> bool {
        let need_enc = Encoding::for_value(value);
        // при необходимости апгрейдим
        if need_enc as u8 > self.encoding as u8 {
            self.upgrade(need_enc);
        }
        let (exists, pos) = self.find(value);
        if exists {
            return false;
        }
        let eb = self.encoding.bytes();
        // вставляем `value` в `data` по байтовому офсету pos*eb
        let mut buf = vec![0u8; eb];
        match self.encoding {
            Encoding::Int16 => buf.copy_from_slice(&(value as i16).to_le_bytes()),
            Encoding::Int32 => buf.copy_from_slice(&(value as i32).to_le_bytes()),
            Encoding::Int64 => buf.copy_from_slice(&value.to_le_bytes()),
        }
        self.data.splice(pos * eb..pos * eb, buf);
        true
    }

    pub fn remove(&mut self, value: i64) -> bool {
        let (exists, pos) = self.find(value);
        if !exists {
            return false;
        }
        let eb = self.encoding.bytes();
        self.data.drain(pos * eb..pos * eb + eb);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        (0..self.len()).map(move |i| self.read_at(i))
    }

    pub fn into_vec(self) -> Vec<i64> {
        self.iter().collect()
    }

    /// Читаем элемент под индексом i с учётом текущего encoding
    fn read_at(&self, i: usize) -> i64 {
        let eb = self.encoding.bytes();
        let start = i * eb;
        let chunk = &self.data[start..start + eb];
        match self.encoding {
            Encoding::Int16 => i16::from_le_bytes(chunk.try_into().unwrap()) as i64,
            Encoding::Int32 => i32::from_le_bytes(chunk.try_into().unwrap()) as i64,
            Encoding::Int64 => i64::from_le_bytes(chunk.try_into().unwrap()),
        }
    }

    /// Перекодируем `data` в новый `encoding`, сохраняя сортировку
    fn upgrade(&mut self, new_enc: Encoding) {
        let mut new = Vec::with_capacity(self.len() * new_enc.bytes());
        for v in self.iter() {
            match new_enc {
                Encoding::Int16 => new.extend_from_slice(&(v as i16).to_le_bytes()),
                Encoding::Int32 => new.extend_from_slice(&(v as i32).to_le_bytes()),
                Encoding::Int64 => new.extend_from_slice(&v.to_le_bytes()),
            }
        }
        self.encoding = new_enc;
        self.data = new;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_insert_remove() {
        let mut s = IntSet::new();
        assert!(s.insert(1));
        assert!(s.insert(1000));
        assert!(s.insert(-500));
        assert!(!s.insert(1)); // дубликат
        assert!(s.contains(1000));
        assert!(!s.contains(2));
        assert_eq!(s.into_vec(), vec![-500, 1, 1000]);
    }

    #[test]
    fn test_upgrade_encoding() {
        let mut s = IntSet::new();
        s.insert(100);
        // пока что Int16
        assert_eq!(s.encoding, Encoding::Int16);
        s.insert(70_000); // > i16::MAX → упаковка в Int32
        assert_eq!(s.encoding, Encoding::Int32);
        assert!(s.contains(100));
        assert!(s.contains(70_000));
        assert_eq!(s.into_vec(), vec![100, 70_000]);
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut s = IntSet::new();
        s.insert(5);
        assert!(!s.remove(10));
        assert!(s.remove(5));
        assert!(s.is_empty());
    }

    #[test]
    fn test_iter_sorted() {
        let mut s = IntSet::new();
        for &v in &[30, -10, 0, 20] {
            s.insert(v);
        }
        let collected: Vec<_> = s.iter().collect();
        assert_eq!(collected, vec![-10, 0, 20, 30]);
    }
}
