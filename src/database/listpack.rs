//! Модуль `ListPack` реализует компактную структуру данных для хранения
//! элементов переменной длины, оптимизированную по использованию памяти и
//! сериализации.
//!
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
    /// Создаёт новый пустой `ListPack` с заранее
    /// выделенной ёмкостью и размещённым по центру
    /// байтом-завершителем.
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

    /// Амортизированное расширение и центрирование
    /// внутреннего буфера при необходимости.
    /// Обеспечивает наличие достаточного пространства
    /// для вставки `extra` байт.
    fn grow_and_center(
        &mut self,
        extra: usize,
    ) {
        let used = self.tail - self.head;
        let need = used + extra + 1;
        if need <= self.data.len() {
            // Пространства достаточно.
            return;
        }
        // Новая ёмкость: максимум между 1.5× текущей и требуемой.
        let new_cap = (self.len().max(1) * 3 / 2).max(need);
        let mut new_data = vec![0; new_cap];
        // Центрируем текущее содержимое в новом буфере.
        let new_head = (new_cap - used) / 2;
        new_data[new_head..new_head + used].copy_from_slice(&self.data[self.head..self.tail]);
        self.head = new_head;
        self.tail = new_head + used;
        self.data = new_data;
    }

    /// Вставляет значение в начало списка.
    pub fn push_front(
        &mut self,
        value: &[u8],
    ) {
        // Кодируем длину значения в формате varint
        let mut len_bytes = Vec::new();
        let mut v = value.len();
        while v >= 0x80 {
            len_bytes.push((v as u8 & 0x7F) | 0x80);
            v >>= 7;
        }
        len_bytes.push(v as u8);

        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        // Сдвигаем head назад и записываем длину + данные
        self.head -= extra;
        let h = self.head;
        // длина
        self.data[h..h + len_bytes.len()].copy_from_slice(&len_bytes);
        // сами байты значения
        self.data[h + len_bytes.len()..h + extra].copy_from_slice(value);

        self.num_entries += 1;
    }

    /// Вставляет значение в конец списка.
    pub fn push_back(
        &mut self,
        value: &[u8],
    ) {
        // Кодируем длину значения в формате varint
        let mut len_bytes = Vec::new();
        let mut v = value.len();
        while v >= 0x80 {
            len_bytes.push((v as u8 & 0x7F) | 0x80);
            v >>= 7;
        }
        len_bytes.push(v as u8);

        let extra = len_bytes.len() + value.len();
        self.grow_and_center(extra);

        // Перезаписываем текущий терминатор (0xFF) на позиции tail-1,
        // затем пишем длину + значение + новый терминатор 0xFF.
        let term_pos = self.tail - 1;
        // длина
        self.data[term_pos..term_pos + len_bytes.len()].copy_from_slice(&len_bytes);
        // сами байты значения
        let vstart = term_pos + len_bytes.len();
        self.data[vstart..vstart + value.len()].copy_from_slice(value);
        // новый терминатор
        let new_term = vstart + value.len();
        self.data[new_term] = 0xFF;

        self.tail = new_term + 1;
        self.num_entries += 1;
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
    pub fn encode_variant(mut value: usize) -> Vec<u8> {
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
}

impl Default for ListPack {
    /// Реализация `Default`, создающая новый пустой `ListPack`.
    fn default() -> Self {
        Self::new()
    }
}
