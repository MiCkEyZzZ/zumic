pub struct ListPack {
    data: Vec<u8>,
}

impl ListPack {
    pub fn new() -> Self {
        let mut data = Vec::with_capacity(64);
        data.push(0xFF); // конец.
        Self { data }
    }

    pub fn push_back(&mut self, value: &[u8]) {
        let len = value.len() as u8;
        assert!(len < 0xFE, "Too long for simple listpack entry");

        let insert_pos = self.data.len() - 1; // перед 0xFF
        self.data.insert(insert_pos, len);
        self.data
            .splice(insert_pos + 1..insert_pos + 1, value.iter().cloned());
    }

    pub fn len(&self) -> usize {
        let mut i = 0;
        let mut count = 0;
        while i < self.data.len() {
            if self.data[i] == 0xFF {
                break;
            }
            let len = self.data[i] as usize;
            i += 1 + len;
            count += 1;
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

            let len = self.data[i] as usize;
            if current == index {
                return Some(&self.data[i + 1..i + 1 + len]);
            }

            i += 1 + len;
            current += 1;
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

            let len = data[i] as usize;
            let start = i + 1;
            let end = start + len;
            i = end;
            Some(&data[start..end])
        })
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
}
