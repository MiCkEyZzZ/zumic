//! `ListPack` — это структура данных, предназначенная для хранения
//! последовательности байтовых элементов. Каждый элемент предваряется длиной,
//! закодированной в формате переменной длины (varint), а весь список
//! завершается специальным байтом 0xFF.

/// Динамический список элементов переменной длины в бинарном формате.
///
/// `ListPack` хранит элементы в виде последовательности байт с кодированием
/// длины каждого элемента в формате varint и терминатором `0xFF`.
/// Поддерживаются вставки и удаление элементов с обеих сторон списка.
#[derive(Clone)]
pub struct ListPack {
    /// Внутренний буфер с элементами и терминатором
    data: Vec<u8>,
    /// Индекс начала первого элемента (или позиции для вставки в начало)
    head: usize,
    /// Индекс конца последнего элемента + терминатор (или позиции для вставки в
    /// конец)
    tail: usize,
    /// Количество элементов в списке
    num_entries: usize,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl ListPack {
    /// Создаёт новый пустой `ListPack`.
    ///
    /// Внутренний буфер инийиализируется с заранее выделенно ёмкостью, а
    /// байт-завершитель (`0xFF`) размещается по центру буфера для обеспечения
    /// эффективных вставок как в начало, так и в конец списка.
    ///
    /// # Инициализация
    /// - Буфер создаётся с начальной ёмкостью по умолчанию;
    /// - `head` и `tail` устанавливаются таким образом, что список изначально
    ///   пуст и корректно терминрован.
    ///
    /// # Гарантии
    /// - После создания `len() == 0`;
    /// - Внутренние инварианты структуры соблюдены.
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
    ///
    /// Значение добавляется перед всеми существующими элементами.
    /// Длина элемента предварительно кодируется в формате varint и сохраняется
    /// вместе с данными во внутреннем буфере.
    pub fn push_front(
        &mut self,
        value: &[u8],
    ) {
        // Кодируем длину значения в формате varint
        let len_bytes = Self::encode_varint(value.len());
        let extra = len_bytes.len() + value.len();

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
    ///
    /// Значение добавляется после всех существующих элементов.
    /// Перед записью данные кодируются во внутреннем бинарном формате: длина
    /// элемента сохраняется в формате varint, за которой следуют сами данные и
    /// терминатор списка.
    pub fn push_back(
        &mut self,
        value: &[u8],
    ) {
        // Кодируем длину значения в формате varint
        let len_bytes = Self::encode_varint(value.len());
        let extra = len_bytes.len() + value.len();

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
    ///
    /// # Возвращает
    /// - `Some(Vec<u8>)` - данные удалённого элемента
    /// - `None` - если список пуст
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
    ///
    /// # Возвращает
    /// - `Some(Vec<u8>)` - данные удалённого элемента
    /// - `None` - если список пуст
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

    /// Очищает список, удаляя все элементы.
    ///
    /// После вызова этого метода список становится пустым.
    ///
    /// Обратите внимание, что этот метод **не освобождает и не уменьшает**
    /// выделенный буфер — внутренняя ёмкость `ListPack` остаётся прежней,
    /// что позволяет повторно использовать память без дополнительных
    /// аллокаций.
    ///
    /// Внутренние указатели начала и конца сбрасываются в исходное состояние,
    /// а в буфер записывается терминатор `0xFF`.
    #[inline]
    pub fn clear(&mut self) {
        let cap = self.data.len();
        self.head = cap / 2;
        self.tail = self.head + 1;
        self.data[self.head] = 0xFF;
        self.num_entries = 0;
    }

    /// Укорачивает список, оставляя первые `len` элементов и удаляя остальные.
    ///
    /// Если `len` больше либо равно текущему кол-ву элементов, метод не
    /// оказывает никакого эффекта.
    ///
    /// Порядок оставшихся элементов сохраняется.
    /// Ёмкость внутреннего буфера при этом **не изменяется**.
    pub fn truncate(
        &mut self,
        len: usize,
    ) {
        if len >= self.num_entries {
            return;
        }

        while self.num_entries > len {
            self.pop_back();
        }
    }

    /// Изменяет размер списка до указанной длины.
    pub fn resize(
        &mut self,
        new_len: usize,
        fill: &[u8],
    ) {
        match new_len.cmp(&self.num_entries) {
            std::cmp::Ordering::Greater => {
                // Добавляем недостающие элементы
                let to_add = new_len - self.num_entries;
                for _ in 0..to_add {
                    self.push_back(fill);
                }
            }
            std::cmp::Ordering::Less => {
                // Удаляем лишние элементы
                self.truncate(new_len);
            }
            std::cmp::Ordering::Equal => {
                // Ничего не делаем
            }
        }
    }

    /// Возвращает количество элементов в списке.
    ///
    /// Это значение также может рассматриваться как *длина* списка.
    #[inline]
    pub fn len(&self) -> usize {
        self.num_entries
    }

    /// Возвращает `true`, если список не содержит элементов.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }

    /// Возвращает ёмкость внетреннуго буфера в байтах.
    ///
    /// Это значение соответствует `data.len()` и показывает, сколько байт
    /// памяти выделено под `ListPack`, а не максимальное кол-во элементов.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// Кол-во байт, занятых элементами и терминатором.
    #[inline]
    pub fn used_bytes(&self) -> usize {
        self.tail - self.head
    }

    /// Кол-во неиспользуемых байт в буфере.
    #[inline]
    pub fn free_bytes(&self) -> usize {
        self.len() - self.used_bytes()
    }

    /// Возвращает ссылку на элемент по указанному индексу, если он существует.
    ///
    /// # Возвращает
    /// - `Some(&[u8])` - срез данных элемента по указанному индексу
    /// - `None` - если индекс выходит за границу списка
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
                return Some(&self.data[i + consumed..i + consumed + len]);
            }
            i += consumed + len;
            curr += 1;
        }

        None
    }

    /// Возвращает итератор по всем элементам списка в порядке хранения.
    ///
    /// # Возвращает
    /// - итератор с элементами типа `&[u8]`, где каждый элемент является
    ///   представлением соответствующего значения списка
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

    /// Возвращает reverse iterator по всем элементам списка.
    pub fn iter_rev(&self) -> impl DoubleEndedIterator<Item = &[u8]> {
        let positions = self.collect_element_positions();
        let data_ptr = self.data.as_ptr();

        positions.into_iter().rev().map(move |(start, len)| unsafe {
            std::slice::from_raw_parts(data_ptr.add(start), len)
        })
    }

    /// Удаляет элемент по указанному индексу.
    ///
    /// # Возвращает:
    /// - `true` - если элемент был успешно удалён
    /// - `false` - если индекс выходит за границы списка
    pub fn remove(
        &mut self,
        index: usize,
    ) -> bool {
        if index >= self.num_entries {
            return false;
        }

        // Оптимизация для первого элемента - О(1) без поиска
        if index == 0 {
            self.pop_front();
            return true;
        }

        // Для остальных случаев нужно найти позицию элемента
        let (elem_start, elem_len, consumed) = match self.find_element_pos(index) {
            Some(pos) => pos,
            None => return false,
        };

        let elem_end = elem_start + consumed + elem_len;

        // Оптимизация для последнего элемента
        if index == self.num_entries - 1 {
            self.tail = elem_start;
            self.data[self.tail] = 0xFF;
            self.tail += 1;
            self.num_entries -= 1;
            return true;
        }

        // Удаление из середины: выбираем направдение копирования
        let left_bytes = elem_start - self.head;
        let right_bytes = self.tail - elem_end;

        if left_bytes < right_bytes {
            // Копируем левую часть вправо (меньше данных)
            // [head ... elem_start] -> [elem_end ...]
            self.data.copy_within(self.head..elem_start, elem_end);
            self.head += elem_end - elem_start;
        } else {
            // Копируем правую часть влево (меньше данных)
            // [elem_end ... tail] -> [elem_start ...]
            self.data.copy_within(elem_end..self.tail, elem_start);
            self.tail -= elem_end - elem_start;
        }

        self.num_entries -= 1;
        true
    }

    /// Кодирует значение `usize` в формате переменной длины (varint).
    ///
    /// Возвращает вектор байт, содержащий varint-представление значения.
    ///
    /// # Гарантии
    /// - Для значений `0..=127` результат содержит ровно один байт;
    /// - Размер результата не превышает `ceil(size_of::<usize>() * 8 / 7)`
    ///   байт.
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

    /// Декодирует целое число в формате переменной длины (varint) из начала
    /// переданного среза байт.
    ///
    /// # Возвращает:
    /// - `value` - декодированное целое число
    /// - `consumed` - кол-во байт, прочитанных из среза
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

    /// Находит позицию элемента по индексу.
    ///
    /// # Возвращает:
    /// - `start` - позиция начала length varint в буфере
    /// - `len` - длина данных элемента
    /// - `consumed` - кол-во байт занятое length varint
    fn find_element_pos(
        &self,
        index: usize,
    ) -> Option<(usize, usize, usize)> {
        if index >= self.num_entries {
            return None;
        }

        let mut pos = self.head;
        let mut curr = 0;

        while pos < self.tail {
            if self.data[pos] == 0xFF {
                break;
            }

            let (len, consumed) = Self::decode_varint(&self.data[pos..])?;

            if curr == index {
                return Some((pos, len, consumed));
            }

            pos += consumed + len;
            curr += 1;
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

    /// Собирает позиции всех элементов списка.
    ///
    /// # Возвращает
    /// - `Vec<(usize, usize)>` — вектор пар `(start, len)`, где:
    ///   - `start` — индекс начала данных элемента в буфере `data`
    ///   - `len` — длина элемента в байтах
    fn collect_element_positions(&self) -> Vec<(usize, usize)> {
        let mut positions = Vec::with_capacity(self.num_entries);
        let mut pos = self.head;

        while pos < self.tail && self.data[pos] != 0xFF {
            if let Some((len, consumed)) = Self::decode_varint(&self.data[pos..]) {
                let start = pos + consumed;
                positions.push((start, len));
                pos = start + len;
            } else {
                break;
            }
        }

        positions
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для ListPack
////////////////////////////////////////////////////////////////////////////////

impl Default for ListPack {
    /// Реализация `Default`, создающая новый пустой `ListPack`.
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет, что `pop_front` на пустом списке возвращает `None`.
    #[test]
    fn test_pop_front_empty() {
        let mut lp = ListPack::new();
        assert_eq!(lp.pop_front(), None);
    }

    /// Тест проверяет, что `pop_back` на пустом списке возвращает `None`.
    #[test]
    fn test_pop_back_empty() {
        let mut lp = ListPack::new();
        assert_eq!(lp.pop_back(), None);
    }

    /// Тест проверяет корректное извлечение единственного элемента через
    /// `pop_front` и сброс состояния списка после опустошения.
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
    fn test_pop_back_single_element() {
        let mut lp = ListPack::new();
        lp.push_back(b"world");

        assert_eq!(lp.len(), 1);
        assert_eq!(lp.pop_back(), Some(b"world".to_vec()));
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_back(), None);
    }

    /// Тест проверяет последовательное удаление элементов из начала списка
    /// и сохранение корректного порядка.
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

    /// Тест проверяет последовательное удаление элементов с конца списка
    /// и корректный LIFO-порядок.
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

    /// Тест проверяет корректную работу смешанных операций `push_front`,
    /// `push_back`, `pop_front` и `pop_back`.
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

    /// Тест проверяет, что после `pop_front` оставшиеся элементы
    /// доступны по корректным индексам.
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

    /// Тест проверяет, что после `pop_back` оставшиеся элементы
    /// сохраняют корректный порядок.
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

    /// Тест проверяет, что при массовом `pop_front` происходит корректное
    /// рецентрирование буфера без потери данных.
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

    /// Тест проверяет корректную работу `pop_front` и `pop_back`
    /// с большими элементами данных.
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

    /// Тест проверяет корректность вставки после `pop_front`.
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

    /// Тест проверяет корректность вставки в начало после `pop_back`.
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

    /// Тест проверяет корректность работы итератора
    /// после последовательных операций удаления.
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

    /// Тест проверяет устойчивость `pop_front` при большом количестве элементов
    /// и сохранение строгого FIFO-порядка.
    #[test]
    fn test_stress_pop_front() {
        let mut lp = ListPack::new();
        let n = 1000;

        for i in 0usize..n {
            lp.push_back(&i.to_le_bytes());
        }

        for i in 0..n {
            let popped = lp.pop_front().expect("should have element");
            let arr: [u8; std::mem::size_of::<usize>()] = popped.try_into().unwrap();
            let value = usize::from_le_bytes(arr);
            assert_eq!(value, i);
        }

        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_front(), None);
    }

    /// Тест проверяет устойчивость `pop_back` при большом количестве элементов
    /// и сохранение строгого LIFO-порядка.
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

    #[test]
    fn test_iter_rev_empty() {
        let lp = ListPack::new();
        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), 0);
    }

    #[test]
    fn test_iter_rev_single_element() {
        let mut lp = ListPack::new();
        lp.push_back(b"only");

        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], b"only");
    }

    #[test]
    fn test_iter_rev_multiple_elements() {
        let mut lp = ListPack::new();
        lp.push_back(b"first");
        lp.push_back(b"second");
        lp.push_back(b"third");

        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], b"third");
        assert_eq!(collected[1], b"second");
        assert_eq!(collected[2], b"first");
    }

    #[test]
    fn test_iter_rev_count_matches_forwatd() {
        let mut lp = ListPack::new();
        for i in 0..10 {
            lp.push_back(&[i]);
        }

        let forward_count = lp.iter().count();
        let reverse_count = lp.iter_rev().count();

        assert_eq!(forward_count, reverse_count);
        assert_eq!(forward_count, 10);
    }

    #[test]
    fn test_iter_rev_reverse_of_forward() {
        let mut lp = ListPack::new();
        for i in 0u8..5 {
            lp.push_back(&[i]);
        }

        let forward: Vec<Vec<u8>> = lp.iter().map(|x| x.to_vec()).collect();
        let reverse: Vec<Vec<u8>> = lp.iter_rev().map(|x| x.to_vec()).collect();

        assert_eq!(forward.len(), reverse.len());
        for (i, fwd_item) in forward.iter().enumerate() {
            let rev_item = &reverse[reverse.len() - 1 - i];
            assert_eq!(fwd_item, rev_item);
        }
    }

    #[test]
    fn test_iter_rev_after_removals() {
        let mut lp = ListPack::new();
        for i in 0..10 {
            lp.push_back(&[i]);
        }

        lp.pop_front();
        lp.pop_back();

        let collected: Vec<Vec<u8>> = lp.iter_rev().map(|x| x.to_vec()).collect();
        assert_eq!(collected.len(), 8);
        assert_eq!(collected[0], vec![8]);
        assert_eq!(collected[7], vec![1]);
    }

    #[test]
    fn test_iter_rev_with_large_elements() {
        let mut lp = ListPack::new();
        let large1 = vec![1u8; 500];
        let large2 = vec![2u8; 500];
        let large3 = vec![3u8; 500];

        lp.push_back(&large1);
        lp.push_back(&large2);
        lp.push_back(&large3);

        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], &large3[..]);
        assert_eq!(collected[1], &large2[..]);
        assert_eq!(collected[2], &large1[..]);
    }

    #[test]
    fn test_iter_rev_double_ended() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.push_back(b"c");
        lp.push_back(b"d");

        let mut rev_iter = lp.iter_rev();

        assert_eq!(rev_iter.next(), Some(b"d".as_ref()));
        assert_eq!(rev_iter.next_back(), Some(b"a".as_ref()));
        assert_eq!(rev_iter.next(), Some(b"c".as_ref()));
        assert_eq!(rev_iter.next_back(), Some(b"b".as_ref()));
        assert_eq!(rev_iter.next(), None);
    }

    #[test]
    fn test_iter_rev_after_clear_and_refill() {
        let mut lp = ListPack::new();
        lp.push_back(b"old1");
        lp.push_back(b"old2");
        lp.clear();

        lp.push_back(b"new1");
        lp.push_back(b"new2");
        lp.push_back(b"new3");

        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], b"new3");
        assert_eq!(collected[2], b"new1");
    }

    #[test]
    fn test_iter_rev_after_truncate() {
        let mut lp = ListPack::new();
        for i in 0..10 {
            lp.push_back(&[i]);
        }

        lp.truncate(5);

        let collected: Vec<Vec<u8>> = lp.iter_rev().map(|x| x.to_vec()).collect();
        assert_eq!(collected.len(), 5);
        assert_eq!(collected[0], vec![4]);
        assert_eq!(collected[4], vec![0]);
    }

    #[test]
    fn test_iter_rev_after_resize_grow() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.resize(5, b"x");

        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), 5);
        assert_eq!(collected[0], b"x");
        assert_eq!(collected[1], b"x");
        assert_eq!(collected[2], b"x");
        assert_eq!(collected[3], b"b");
        assert_eq!(collected[4], b"a");
    }

    #[test]
    fn test_multiple_iter_rev() {
        let mut lp = ListPack::new();
        lp.push_back(b"1");
        lp.push_back(b"2");
        lp.push_back(b"3");

        let iter1 = lp.iter_rev();
        let iter2 = lp.iter_rev();

        let collected1: Vec<&[u8]> = iter1.collect();
        let collected2: Vec<&[u8]> = iter2.collect();

        assert_eq!(collected1, collected2);
    }

    #[test]
    fn test_forward_and_reverse_iteration_combined() {
        let mut lp = ListPack::new();
        for i in 0..5 {
            lp.push_back(&[i]);
        }

        let forward: Vec<Vec<u8>> = lp.iter().map(|x| x.to_vec()).collect();
        let reverse: Vec<Vec<u8>> = lp.iter_rev().map(|x| x.to_vec()).collect();

        assert_eq!(forward[0], reverse[4]);
        assert_eq!(forward[1], reverse[3]);
        assert_eq!(forward[2], reverse[2]);
        assert_eq!(forward[3], reverse[1]);
        assert_eq!(forward[4], reverse[0]);
    }

    #[test]
    fn test_stress_iter_rev() {
        let mut lp = ListPack::new();
        let n = 1000;

        for i in 0usize..n {
            lp.push_back(&i.to_le_bytes());
        }

        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), n);

        for (i, elem) in collected.iter().enumerate() {
            let expected = n - 1 - i;
            let value = usize::from_le_bytes((*elem).try_into().unwrap());
            assert_eq!(value, expected);
        }
    }

    #[test]
    fn test_iter_rev_variable_length_elements() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"bb");
        lp.push_back(b"ccc");
        lp.push_back(b"dddd");

        let collected: Vec<&[u8]> = lp.iter_rev().collect();
        assert_eq!(collected.len(), 4);
        assert_eq!(collected[0], b"dddd");
        assert_eq!(collected[1], b"ccc");
        assert_eq!(collected[2], b"bb");
        assert_eq!(collected[3], b"a");
    }

    /// Тест проверяет корректную очистку пустого списка через `clear`
    /// и сохранение инвариантов буфера.
    #[test]
    fn test_clear_empty_list() {
        let mut lp = ListPack::new();
        let cap = lp.data.len();
        lp.clear();
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_front(), None);
        assert_eq!(lp.pop_back(), None);
        assert_eq!(lp.data.len(), cap);
        assert_eq!(lp.head, cap / 2);
        assert_eq!(lp.data[lp.head], 0xFF);
    }

    /// Тест проверяет, что `clear` полностью очищает непустой список
    /// и корректно сбрасывает состояние.
    #[test]
    fn test_clear_non_empty_list() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        assert_eq!(lp.len(), 2);
        lp.clear();
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.get(0), None);
        assert_eq!(lp.pop_front(), None);
        assert_eq!(lp.pop_back(), None);
        assert_eq!(lp.data[lp.head], 0xFF);
    }

    /// Тест проверяет возможность повторного использования списка
    /// после вызова `clear`.
    #[test]
    fn test_clear_then_reuse() {
        let mut lp = ListPack::new();
        lp.push_back(b"x");
        lp.clear();
        lp.push_back(b"y");
        assert_eq!(lp.len(), 1);
        assert_eq!(lp.pop_front(), Some(b"y".to_vec()));
    }

    /// Тест проверяет, что `clear` не изменяет ёмкость буфера
    /// и корректно восстанавливает `head`, `tail` и `num_entries`.
    #[test]
    fn test_clear_reuses_buffer() {
        let mut lp = ListPack::new();
        let original_cap = lp.data.len();
        for i in 0..10 {
            lp.push_back(&[i]);
        }
        lp.clear();
        assert_eq!(lp.data.len(), original_cap);
        assert_eq!(lp.head, original_cap / 2);
        assert_eq!(lp.tail, lp.head + 1);
        assert_eq!(lp.num_entries, 0);
    }

    /// Тест проверяет, что `truncate` с длиной больше текущей
    /// не изменяет содержимое списка.
    #[test]
    fn test_truncate_no_op_when_len_greater() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.push_back(b"c");
        let before = lp.len();
        lp.truncate(10);
        assert_eq!(lp.len(), before);
        assert_eq!(lp.get(0), Some(b"a".as_ref()));
        assert_eq!(lp.get(2), Some(b"c".as_ref()));
    }

    /// Тест проверяет, что `truncate` с длиной, равной текущей,
    /// является no-op.
    #[test]
    fn test_truncate_no_op_when_len_equal() {
        let mut lp = ListPack::new();
        lp.push_back(b"1");
        lp.push_back(b"2");
        let before = lp.len();
        lp.truncate(before);
        assert_eq!(lp.len(), before);
        assert_eq!(lp.get(0), Some(b"1".as_ref()));
        assert_eq!(lp.get(1), Some(b"2".as_ref()));
    }

    /// Тест проверяет корректное удаление хвостовых элементов
    /// при уменьшении длины через `truncate`.
    #[test]
    fn test_truncate_to_smaller_length() {
        let mut lp = ListPack::new();
        for ch in b'0'..=b'4' {
            lp.push_back(&[ch]);
        }
        assert_eq!(lp.len(), 5);
        lp.truncate(2);
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(&[b'0'][..]));
        assert_eq!(lp.get(1), Some(&[b'1'][..]));
        assert_eq!(lp.get(2), None);
    }

    /// Тест проверяет, что `truncate(0)` полностью очищает список
    /// и корректно сбрасывает внутреннее состояние.
    #[test]
    fn test_truncate_to_zero() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.truncate(0);
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_front(), None);
        assert_eq!(lp.pop_back(), None);
        // head/tail восстановлены
        let cap = lp.data.len();
        assert_eq!(lp.head, cap / 2);
        assert_eq!(lp.tail, lp.head + 1);
    }

    /// Тест проверяет расширение списка через `resize`
    /// с заполнением значением `fill`.
    #[test]
    fn test_resize_grow() {
        let mut lp = ListPack::new();
        lp.push_back(b"one");
        lp.push_back(b"two");
        lp.resize(5, b"x");
        assert_eq!(lp.len(), 5);
        assert_eq!(lp.get(0), Some(b"one".as_ref()));
        assert_eq!(lp.get(1), Some(b"two".as_ref()));
        assert_eq!(lp.get(2), Some(b"x".as_ref()));
        assert_eq!(lp.get(4), Some(b"x".as_ref()));
    }

    /// Тест проверяет корректное сокращение списка
    /// при `resize` в меньшую сторону.
    #[test]
    fn test_resize_shrink() {
        let mut lp = ListPack::new();
        lp.push_back(b"a");
        lp.push_back(b"b");
        lp.push_back(b"c");
        lp.resize(1, b"x");
        assert_eq!(lp.len(), 1);
        assert_eq!(lp.get(0), Some(b"a".as_ref()));
        assert_eq!(lp.get(1), None);
    }

    /// Тест проверяет, что `resize` с той же длиной
    /// не изменяет список.
    #[test]
    fn test_resize_no_change() {
        let mut lp = ListPack::new();
        lp.push_back(b"alpha");
        let before = lp.len();
        lp.resize(before, b"fill");
        assert_eq!(lp.len(), before);
        assert_eq!(lp.get(0), Some(b"alpha".as_ref()));
    }

    /// Тест проверяет, что `resize(0)` полностью очищает список.
    #[test]
    fn test_resize_to_zero() {
        let mut lp = ListPack::new();
        lp.push_back(b"z");
        lp.resize(0, b"fill");
        assert_eq!(lp.len(), 0);
        assert_eq!(lp.pop_front(), None);
    }

    /// Тест проверяет корректное расширение пустого списка
    /// через `resize`.
    #[test]
    fn test_resize_from_empty() {
        let mut lp = ListPack::new();
        lp.resize(3, b"F");
        assert_eq!(lp.len(), 3);
        assert_eq!(lp.get(0), Some(b"F".as_ref()));
        assert_eq!(lp.get(2), Some(b"F".as_ref()));
    }

    /// Тест проверяет работу `resize` с большими элементами данных.
    #[test]
    fn test_resize_with_large_fill() {
        let mut lp = ListPack::new();
        let big_fill = vec![7u8; 300];
        lp.resize(2, &big_fill);
        assert_eq!(lp.len(), 2);
        assert_eq!(lp.get(0), Some(&big_fill[..]));
        assert_eq!(lp.get(1), Some(&big_fill[..]));
    }
}
