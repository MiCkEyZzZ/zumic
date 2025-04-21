//! Модуль `ListPack` реализует компактную структуру хранения элементов
//! с переменной длиной, оптимизированную для минимального использования
//! памяти и сериализации.
//!
//! `ListPack` — это структура данных, аналогичная, хранящая
//! последовательность байтовых элементов. Каждый элемент предваряется
//! длиной, закодированной в формате с переменной длиной (varint), и
//! завершается специальным байтом 0xFF.

pub struct ListPack {
    data: Vec<u8>,
}

impl ListPack {
    pub fn new() -> Self {
        let mut data = Vec::with_capacity(64);
        data.push(0xFF); // конец.
        Self { data }
    }

    pub fn push_front(&mut self, value: &[u8]) {
        let len_bytes = Self::encode_variant(value.len());
        let insert_pos = 0;

        self.data
            .splice(insert_pos..insert_pos, value.iter().cloned());
        self.data
            .splice(insert_pos..insert_pos, len_bytes.iter().cloned());
    }

    pub fn push_back(&mut self, value: &[u8]) {
        let len_bytes = Self::encode_variant(value.len());
        let insert_pos = self.data.len() - 1; // перед 0xFF

        // вставляем длину
        self.data
            .splice(insert_pos..insert_pos, len_bytes.iter().cloned());
        // вставляем значение
        self.data.splice(
            insert_pos + len_bytes.len()..insert_pos + len_bytes.len(),
            value.iter().cloned(),
        );
    }

    pub fn len(&self) -> usize {
        let mut i = 0;
        let mut count = 0;

        while i < self.data.len() {
            if self.data[i] == 0xFF {
                break;
            }

            match Self::decode_variant(&self.data[i..]) {
                Some((len, consumed)) => {
                    i += consumed + len;
                    count += 1;
                }
                None => break,
            }
        }
        count
    }

    pub fn get(&self, index: usize) -> Option<&[u8]> {
        let mut i = 0;
        let mut current = 0;
        while i < self.data.len() {
            if self.data[i] == 0xFF {
                break;
            }

            match Self::decode_variant(&self.data[i..]) {
                Some((len, consumed)) => {
                    if current == index {
                        return Some(&self.data[i + consumed..i + consumed + len]);
                    }
                    i += consumed + len;
                    current += 1;
                }
                None => break,
            }
        }
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        let mut i = 0;
        let data = &self.data;
        std::iter::from_fn(move || {
            if i >= data.len() || data[i] == 0xFF {
                return None;
            }

            match ListPack::decode_variant(&data[i..]) {
                Some((len, consumed)) => {
                    let start = i + consumed;
                    let end = start + len;
                    i = end;
                    Some(&data[start..end])
                }
                None => None,
            }
        })
    }

    pub fn remove(&mut self, index: usize) -> bool {
        let mut i = 0;
        let mut current = 0;
        while i < self.data.len() {
            if self.data[i] == 0xFF {
                break;
            }

            match Self::decode_variant(&self.data[i..]) {
                Some((len, consumed)) => {
                    if current == index {
                        let end = i + consumed + len;
                        self.data.drain(i..end);
                        return true;
                    }
                    i += consumed + len;
                    current += 1;
                }
                None => break,
            }
        }
        false
    }

    pub fn encode_variant(mut value: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                buf.push(byte);
                break;
            } else {
                buf.push(byte | 0x80); // установить бит продолжения.
            }
        }
        buf
    }

    pub fn decode_variant(bytes: &[u8]) -> Option<(usize, usize)> {
        let mut result = 0usize;
        let mut shift = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            let part = (byte & 0x7F) as usize;
            result |= part << shift;

            if byte & 0x80 == 0 {
                return Some((result, i + 1));
            }

            shift += 7;
            if shift > (std::mem::size_of::<usize>() * 8) {
                return None; // переполнение
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_listpack() {
        let lp = ListPack::new();
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.get(0), None);
        assert_eq!(lp.iter().count(), 0);
    }

    #[test]
    fn test_push_and_len() {
        let mut lp = ListPack::new();
        lp.push_back(b"one");
        lp.push_back(b"two");
        lp.push_back(b"three");

        assert_eq!(lp.len(), 3);
    }

    #[test]
    fn test_get_elements() {
        let mut lp = ListPack::new();
        lp.push_back(b"alpha");
        lp.push_back(b"beta");
        lp.push_back(b"gamma");

        assert_eq!(lp.get(0), Some(b"alpha".as_ref()));
        assert_eq!(lp.get(1), Some(b"beta".as_ref()));
        assert_eq!(lp.get(2), Some(b"gamma".as_ref()));
        assert_eq!(lp.get(3), None);
    }

    #[test]
    fn test_iter() {
        let mut lp = ListPack::new();
        lp.push_back(b"x");
        lp.push_back(b"y");
        lp.push_back(b"z");

        let collected: Vec<_> = lp.iter().map(|e| std::str::from_utf8(e).unwrap()).collect();
        assert_eq!(collected, vec!["x", "y", "z"]);
    }

    #[test]
    fn test_large_element() {
        let mut lp = ListPack::new();

        let large_data = vec![b'a'; 200]; // > 127 байт.
        lp.push_back(&large_data);

        assert_eq!(lp.len(), 1);

        let result = lp.get(0).unwrap();
        assert_eq!(result.len(), 200);
        assert!(result.iter().all(|&b| b == b'a'));
    }

    #[test]
    fn test_multiple_large_elements() {
        let mut lp = ListPack::new();

        for i in 0..5 {
            let data = vec![b'a' + (i as u8); 150 + i * 10]; // растущие > 127 элементы.
            lp.push_back(&data);
        }

        assert_eq!(lp.len(), 5);

        for i in 0..5 {
            let expected = vec![b'a' + (i as u8); 150 + i * 10];
            let actual = lp.get(i).unwrap();
            assert_eq!(actual, expected.as_slice());
        }

        let collected: Vec<usize> = lp.iter().map(|e| e.len()).collect();
        assert_eq!(collected, vec![150, 160, 170, 180, 190]);
    }

    #[test]
    fn test_push_front() {
        let mut lp = ListPack::new();
        lp.push_front(b"second");
        lp.push_front(b"first");

        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(b"first".as_ref()));
        assert_eq!(lp.get(1), Some(b"second".as_ref()));

        let collected: Vec<_> = lp.iter().map(|e| std::str::from_utf8(e).unwrap()).collect();
        assert_eq!(collected, vec!["first", "second"]);
    }

    #[test]
    fn test_remove_middle() {
        let mut lp = ListPack::new();
        lp.push_back(b"one");
        lp.push_back(b"two");
        lp.push_back(b"three");

        let removed = lp.remove(1);
        assert!(removed);
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(b"one".as_ref()));
        assert_eq!(lp.get(1), Some(b"three".as_ref()));
        assert_eq!(lp.get(2), None);
    }

    #[test]
    fn test_remove_invalid_index() {
        let mut lp = ListPack::new();
        lp.push_back(b"only");

        let removed = lp.remove(5);
        assert!(!removed);
        assert_eq!(lp.len(), 1);
    }

    #[test]
    fn test_encode_decode_variant() {
        let test_values = [0, 1, 127, 128, 255, 300, 16384, usize::MAX / 2];

        for &val in &test_values {
            let encoded = ListPack::encode_variant(val);
            let decoded = ListPack::decode_variant(&encoded);
            assert_eq!(decoded, Some((val, encoded.len())));
        }
    }

    #[test]
    fn test_decode_variant_overflow() {
        // Слишком длинное значение - исскуственно создаём "переполнение".
        let bogus = vec![0xFF; 20]; // каждый байт продолжает.
        let result = ListPack::decode_variant(&bogus);
        assert_eq!(result, None);
    }
}
