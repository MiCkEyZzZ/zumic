//! `ListPack` — это структура данных, предназначенная для хранения
//! последовательности байтовых элементов. Каждый элемент предваряется длиной,
//! закодированной в формате переменной длины (varint), а весь список
//! завершается специальным байтом 0xFF.

pub struct ListPack {
    data: Vec<u8>,
    head: usize,
    tail: usize,
    num_entries: usize,
}

impl ListPack {
    /// Создаёт новый пустой `ListPack` с заранее выделенной ёмкостью и
    /// размещённым по центру байтом-завершителем.
    pub fn new() -> Self {
        let cap = 1024;
        let mut data = vec![0; cap];
        let head = cap / 2;
        data[head] = 0xFF;
        Self {
            data,
            head,
            tail: head + 1,
            num_entries: 0,
        }
    }

    /// Вставляет значение в начало списка.
    pub fn push_front(
        &mut self,
        value: &[u8],
    ) {
        // Кодируем длину значения в формате varint
        let len_bytes = Self::encode_varint(value.len());

        let extra = len_bytes.len() + value.len();

        // ВАЖНО: grow_and_center должен гарантировать достаточно места ДО записи
        self.grow_and_center(extra);

        // Сдвигаем head назад и записываем длину + данные
        self.head -= extra;
        let h = self.head;

        // Записываем длину
        self.data[h..h + len_bytes.len()].copy_from_slice(&len_bytes);

        // Записываем данные элемента
        self.data[h + len_bytes.len()..h + extra].copy_from_slice(value);

        self.num_entries += 1;
    }

    /// Вставляет значение в конец списка.
    pub fn push_back(
        &mut self,
        value: &[u8],
    ) {
        // Кодируем длину значения в формате varint
        let len_bytes = Self::encode_varint(value.len());

        let extra = len_bytes.len() + value.len();

        // ВАЖНО: grow_and_center нужно вызвать ДО любых манипуляций с буфером
        self.grow_and_center(extra);

        // Перезаписываем текущий терминатор (0xFF) на позиции tail-1,
        // затем пишем длину + значение + новый терминатор 0xFF.
        let term_pos = self.tail - 1;

        // Записываем длину
        self.data[term_pos..term_pos + len_bytes.len()].copy_from_slice(&len_bytes);

        // Записываем данные элемента
        let vstart = term_pos + len_bytes.len();
        self.data[vstart..vstart + value.len()].copy_from_slice(value);

        // Записываем новый терминатор
        let new_term = vstart + value.len();
        self.data[new_term] = 0xFF;

        self.tail = new_term + 1;
        self.num_entries += 1;
    }

    /// Удаляет и возвращает элемент из начала списка.
    pub fn pop_front(&mut self) -> Option<Vec<u8>> {
        if self.num_entries == 0 {
            return None;
        }

        // Декодируем длину элемента
        let (len, consumed) = Self::decode_varint(&self.data[self.head..])?;
        let start = self.head + consumed;
        let element = self.data[start..start + len].to_vec();

        // Сдвигаем head вперед — O(1) операция
        self.head = start + len;
        self.num_entries -= 1;

        // Если список стал пустым, reset к начальному состоянию
        if self.num_entries == 0 {
            let cap = self.data.len();
            self.head = cap / 2;
            self.tail = self.head + 1;
            self.data[self.head] = 0xFF;
            return Some(element);
        }

        // Рецентрируем если накопилось слишком много неиспользуемого пространства
        // Порог: если head сдвинулся больше чем на 50% от capacity
        if self.head > self.data.len() / 2 {
            self.recenter();
        }

        Some(element)
    }

    /// Удаляет и возвращает элемент из конца списка.
    pub fn pop_back(&mut self) -> Option<Vec<u8>> {
        if self.num_entries == 0 {
            return None;
        }

        // Для нахождения последнего элемента нужно просканировать все элементы
        // TODO: Issue #L5 - добавить backlen encoding для O(1) reverse traversal

        let mut pos = self.head;
        let mut last_start = pos;
        let mut last_len = 0;
        let mut last_consumed = 0;

        // Сканируем все элементы чтобы найти последний
        while pos < self.tail && self.data[pos] != 0xFF {
            last_start = pos;

            let (len, consumed) = Self::decode_varint(&self.data[pos..])?;
            last_len = len;
            last_consumed = consumed;

            pos = pos + consumed + len;
        }

        // Извлекаем данные последнего элемента
        let data_start = last_start + last_consumed;
        let element = self.data[data_start..data_start + last_len].to_vec();

        // Сдвигаем tail назад
        self.tail = last_start;
        self.num_entries -= 1;

        // Если список стал пустым, reset
        if self.num_entries == 0 {
            let cap = self.data.len();
            self.head = cap / 2;
            self.tail = self.head + 1;
            self.data[self.head] = 0xFF;
            return Some(element);
        }

        // Устанавливаем новый терминатор
        self.data[self.tail] = 0xFF;
        self.tail += 1;

        // Рецентрируем если tail слишком близко к концу буфера
        if self.tail > self.data.len() * 3 / 4 {
            self.recenter();
        }

        Some(element)
    }

    /// Возвращает количество элементов в списке.
    pub fn len(&self) -> usize {
        self.num_entries
    }

    /// Возвращает `true`, если список пуст.
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    /// Возвращает ссылку на элемент по указанному индексу, если он существует.
    pub fn get(
        &self,
        index: usize,
    ) -> Option<&[u8]> {
        if index >= self.num_entries {
            return None;
        }

        let mut i = self.head;
        let mut curr = 0;

        while i < self.tail {
            if self.data[i] == 0xFF {
                break;
            }

            let (len, consumed) = Self::decode_varint(&self.data[i..])?;
            if curr == index {
                // возвращаем срез с данными элемента
                return Some(&self.data[i + consumed..i + consumed + len]);
            }
            i += consumed + len;
            curr += 1;
        }

        None
    }

    /// Возвращает итератор по всем элементам списка.
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        let data = &self.data;
        let mut pos = self.head;
        let end = self.tail;

        std::iter::from_fn(move || {
            if pos >= end || data[pos] == 0xFF {
                return None;
            }

            let (len, consumed) = ListPack::decode_varint(&data[pos..])?;
            let start = pos + consumed;
            let slice = &data[start..start + len];
            pos = start + len;
            Some(slice)
        })
    }

    /// Удаляет элемент по указанному индексу. Возвращает `true`, если удаление
    /// прошло успешно.
    pub fn remove(
        &mut self,
        index: usize,
    ) -> bool {
        if index >= self.num_entries {
            return false;
        }

        let mut i = self.head;
        let mut curr = 0;

        while i < self.tail {
            if self.data[i] == 0xFF {
                break;
            }

            if let Some((len, consumed)) = Self::decode_varint(&self.data[i..]) {
                if curr == index {
                    let start = i;
                    let end = i + consumed + len;

                    // Сдвигаем всё после `end` влево на позицию `start`
                    let _to_move = self.tail - end;
                    self.data.copy_within(end..self.tail, start);
                    self.tail -= end - start;

                    // Обновляем терминатор
                    if self.tail > 0 {
                        self.data[self.tail - 1] = 0xFF;
                    }

                    self.num_entries -= 1;
                    return true;
                }

                i += consumed + len;
                curr += 1;
            } else {
                break;
            }
        }

        false
    }

    /// Кодирует значение `usize` в формате переменной длины (varint).
    pub fn encode_varint(mut value: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8; // младшие 7 бит
            value >>= 7;
            if value == 0 {
                buf.push(byte); // последний байт без бита продолжения
                break;
            } else {
                buf.push(byte | 0x80); // устанавливаем бит продолжения
            }
        }
        buf
    }

    /// Декодирует целое число в формате переменной длины (varint) из заданного
    /// среза. Возвращает пару (значение, количество прочитанных байт).
    pub fn decode_varint(data: &[u8]) -> Option<(usize, usize)> {
        let mut result = 0usize;
        let mut shift = 0;
        for (i, &byte) in data.iter().enumerate() {
            result |= ((byte & 0x7F) as usize) << shift;
            if byte & 0x80 == 0 {
                return Some((result, i + 1));
            }
            shift += 7;
            if shift > std::mem::size_of::<usize>() * 8 {
                return None;
            }
        }
        None
    }

    /// Амортизированное расширение и центрирование внутреннего буфера при
    /// необходимости. Обеспечивает наличие достаточного пространства
    /// для вставки `extra` байт.
    fn grow_and_center(
        &mut self,
        extra: usize,
    ) {
        let used = self.tail - self.head;
        let need = used + extra;

        // Проверяем нужно ли вообще расширять
        // Нужно учесть: можем вставлять в начало (head - extra) или в конец (tail +
        // extra)
        let space_before = self.head;
        let space_after = self.data.len() - self.tail;

        // Если достаточно места с обеих сторон, ничего не делаем
        if space_before >= extra && space_after >= extra {
            return;
        }

        // Если нужно больше места - расширяем и центрируем
        let new_cap = (self.data.len().max(1) * 3 / 2).max(need * 2);
        let mut new_data = vec![0; new_cap];

        // Центрируем текущее содержимое в новом буфере
        let new_head = (new_cap - used) / 2;
        new_data[new_head..new_head + used].copy_from_slice(&self.data[self.head..self.tail]);

        self.head = new_head;
        self.tail = new_head + used;
        self.data = new_data;
    }

    /// Пересцентрирование данных в буфере для оптимального использования памяти
    /// при смещении head или tail.
    fn recenter(&mut self) {
        let used = self.tail - self.head;
        if used == 0 {
            // Пустой список — просто reset
            self.head = self.data.len() / 2;
            self.tail = self.head + 1;
            self.data[self.head] = 0xFF;
            return;
        }

        let new_head = (self.data.len() - used) / 2;
        if new_head == self.head {
            return;
        }

        // Копируем данные на новую позицию
        if new_head < self.head {
            // Копируем влево
            self.data.copy_within(self.head..self.tail, new_head);
        } else {
            // Копируем вправо (нужно начинать с конца чтобы избежать overlap)
            for i in (0..used).rev() {
                self.data[new_head + i] = self.data[self.head + i];
            }
        }

        self.head = new_head;
        self.tail = new_head + used;
    }
}

impl Default for ListPack {
    /// Реализация `Default`, создающая новый пустой `ListPack`.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pop_front_empty() {
        let mut lp = ListPack::new();
        assert_eq!(lp.pop_front(), None);
    }

    #[test]
    fn test_pop_back_empty() {
        let mut lp = ListPack::new();
        assert_eq!(lp.pop_back(), None);
    }

    #[test]
    fn test_pop_front_single_element() {
        let mut lp = ListPack::new();
        lp.push_back(b"hello");

        assert_eq!(lp.len(), 1);
        assert_eq!(lp.pop_front(), Some(b"hello".to_vec()));
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_front(), None);
    }

    #[test]
    fn test_pop_front_multiple_elements() {
        let mut lp = ListPack::new();
        lp.push_back(b"first");
        lp.push_back(b"second");
        lp.push_back(b"third");

        assert_eq!(lp.len(), 3);
        assert_eq!(lp.pop_front(), Some(b"first".to_vec()));
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.pop_front(), Some(b"second".to_vec()));
        assert_eq!(lp.len(), 1);
        assert_eq!(lp.pop_front(), Some(b"third".to_vec()));
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_front(), None);
    }

    #[test]
    fn test_pop_back_multiple_elements() {
        let mut lp = ListPack::new();
        lp.push_back(b"first");
        lp.push_back(b"second");
        lp.push_back(b"third");

        assert_eq!(lp.len(), 3);
        assert_eq!(lp.pop_back(), Some(b"third".to_vec()));
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.pop_back(), Some(b"second".to_vec()));
        assert_eq!(lp.len(), 1);
        assert_eq!(lp.pop_back(), Some(b"first".to_vec()));
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_back(), None);
    }

    #[test]
    fn test_push_pop_mixed() {
        let mut lp = ListPack::new();

        lp.push_back(b"1");
        lp.push_back(b"2");
        lp.push_front(b"0");

        assert_eq!(lp.pop_front(), Some(b"0".to_vec()));
        assert_eq!(lp.pop_back(), Some(b"2".to_vec()));
        assert_eq!(lp.pop_front(), Some(b"1".to_vec()));
        assert_eq!(lp.pop_front(), None);
    }

    #[test]
    fn test_pop_front_maintains_remaining_elements() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.push_back(b"c");

        lp.pop_front();

        assert_eq!(lp.get(0), Some(b"b".as_ref()));
        assert_eq!(lp.get(1), Some(b"c".as_ref()));
    }

    #[test]
    fn test_pop_back_maintains_remaining_elements() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.push_back(b"c");

        lp.pop_back();

        assert_eq!(lp.get(0), Some(b"a".as_ref()));
        assert_eq!(lp.get(1), Some(b"b".as_ref()));
    }

    #[test]
    fn test_pop_front_recentering() {
        let mut lp = ListPack::new();

        // Заполняем список
        for i in 0..100 {
            lp.push_back(&[i]);
        }

        let _initial_head = lp.head;

        // Удаляем много элементов спереди
        for _ in 0..60 {
            lp.pop_front();
        }

        // head должен был сдвинуться и возможно произошло рецентрирование
        // Проверяем что данные всё ещё доступны
        assert_eq!(lp.len(), 40);
        assert_eq!(lp.get(0), Some(&[60][..]));

        // После рецентрирования head не должен быть слишком близко к концу
        assert!(lp.head < lp.data.len() * 3 / 4);
    }

    #[test]
    fn test_pop_operations_with_large_elements() {
        let mut lp = ListPack::new();

        let large_data = vec![42u8; 1000];
        lp.push_back(&large_data);
        lp.push_back(b"small");
        lp.push_back(&large_data);

        assert_eq!(lp.pop_front(), Some(large_data.clone()));
        assert_eq!(lp.pop_back(), Some(large_data.clone()));
        assert_eq!(lp.pop_front(), Some(b"small".to_vec()));
        assert_eq!(lp.len(), 0);
    }

    #[test]
    fn test_pop_front_then_push_back() {
        let mut lp = ListPack::new();

        lp.push_back(b"1");
        lp.push_back(b"2");
        lp.pop_front();
        lp.push_back(b"3");

        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(b"2".as_ref()));
        assert_eq!(lp.get(1), Some(b"3".as_ref()));
    }

    #[test]
    fn test_pop_back_then_push_front() {
        let mut lp = ListPack::new();

        lp.push_back(b"1");
        lp.push_back(b"2");
        lp.pop_back();
        lp.push_front(b"0");

        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(b"0".as_ref()));
        assert_eq!(lp.get(1), Some(b"1".as_ref()));
    }

    #[test]
    fn test_iterator_after_pop_operations() {
        let mut lp = ListPack::new();

        for i in 0..10 {
            lp.push_back(&[i]);
        }

        lp.pop_front();
        lp.pop_front();
        lp.pop_back();

        let collected: Vec<Vec<u8>> = lp.iter().map(|x| x.to_vec()).collect();
        assert_eq!(collected.len(), 7);
        assert_eq!(collected[0], vec![2]);
        assert_eq!(collected[6], vec![8]);
    }

    #[test]
    fn test_stress_pop_front() {
        let mut lp = ListPack::new();
        let n = 1000;

        for i in 0usize..n {
            lp.push_back(&i.to_le_bytes());
        }

        for i in 0..n {
            let popped = lp.pop_front().expect("should have element");
            let value = usize::from_le_bytes(popped.try_into().unwrap());
            assert_eq!(value, i);
        }

        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_front(), None);
    }

    #[test]
    fn test_stress_pop_back() {
        let mut lp = ListPack::new();
        let n = 1000;

        for i in 0usize..n {
            lp.push_back(&i.to_le_bytes());
        }

        for i in (0..n).rev() {
            let popped = lp.pop_back().expect("should have element");
            let value = usize::from_le_bytes(popped.try_into().unwrap());
            assert_eq!(value, i);
        }

        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_back(), None);
    }
}
