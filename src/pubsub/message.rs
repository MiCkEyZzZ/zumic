use std::sync::Arc;

use bytes::Bytes;

use super::intern_channel;

/// Представляет одно сообщение в системе pub/sub.
///
/// Содержит:
/// - имя канала, через который отправлено сообщение;
/// - полезную нагрузку сообщения в виде байтов.
///
/// Используется как для обычных подписок (`SUBSCRIBE`),
/// так и для подписок по шаблону (`PSUBSCRIBE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Канал, в который было опубликовано сообщение.
    pub channel: Arc<str>,
    /// Содержимое сообщения.
    pub payload: Bytes,
}

impl Message {
    /// Создаёт новое сообщение из канала и полезной нагрузки.
    ///
    /// Параметр `channel` может быть любого типа, реализующего `AsRef<str>` (`&str`, `String`, `Arc<str>`),
    /// а `payload` — любого типа, преобразуемого в `Bytes` (`Vec<u8>`, `&[u8]`, `Bytes`).
    pub fn new<S, P>(channel: S, payload: P) -> Self
    where
        S: AsRef<str>,
        P: Into<Bytes>,
    {
        Message {
            channel: intern_channel(channel),
            payload: payload.into(),
        }
    }

    /// Создаёт сообщение из полностью статических данных без копирования.
    ///
    /// Это самый быстрый способ создать сообщение, если и канал, и содержимое имеют
    /// статическую область видимости (`'static`).
    pub fn from_static(channel: &'static str, payload: &'static [u8]) -> Self {
        Self {
            channel: intern_channel(channel),
            payload: Bytes::from_static(payload),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверяет создание сообщения из строки и вектора: правильность канала
    /// и преобразование полезной нагрузки в `Bytes`.
    #[test]
    fn test_from_and_vec() {
        let ch = "news";
        let pl_vec = vec![1, 2, 3];
        let msg: Message = Message::new(ch, pl_vec.clone());
        assert_eq!(&*msg.channel, "news");
        assert_eq!(msg.payload, Bytes::from(pl_vec));
    }

    /// Проверяет создание из `String` и `Bytes`, включая совпадение ссылок и содержимого.
    #[test]
    fn new_from_string_and_bytes() {
        let ch_string = String::from("updates");
        let pl_bytes = Bytes::from_static(b"hello");
        let msg: Message = Message::new(ch_string.clone(), pl_bytes.clone());
        assert_eq!(&*msg.channel, &ch_string);
        assert_eq!(msg.payload, pl_bytes);
    }

    /// Проверяет, что клонирование сохраняет указатели (Arc и Bytes) без копирования.
    #[test]
    fn clone_preserves_arc_and_bytes_zero_copy() {
        let msg1 = Message::new("chan", Bytes::from_static(b"x"));
        let arc_ptr = Arc::as_ptr(&msg1.channel);
        let bytes_ptr = msg1.payload.as_ptr();

        let msg2 = msg1.clone();
        assert_eq!(Arc::as_ptr(&msg2.channel), arc_ptr);
        assert_eq!(msg2.payload.as_ptr(), bytes_ptr);
    }

    /// Проверяет создание из статических данных без копирования (`from_static`).
    #[test]
    fn from_static_zero_copy() {
        let msg = Message::from_static("static_chan", b"data");
        assert_eq!(&*msg.channel, "static_chan");
        assert_eq!(msg.payload, Bytes::from_static(b"data"));
    }

    /// Сравнивает поведение `new` и `from_static`: каналы равны по значению,
    /// а указатели совпадают из-за интернирования.
    #[test]
    fn mix_new_and_from_static() {
        let m1 = Message::new("kin", b"dzadza".to_vec());
        let m2 = Message::from_static("kin", b"dzadza");
        assert_eq!(&*m1.channel, &*m2.channel);
        assert_eq!(m1.payload, m2.payload);
        assert!(Arc::ptr_eq(&m1.channel, &m2.channel));
    }

    /// Проверяет корректную работу с пустым каналом и полезной нагрузкой (new и from_static).
    #[test]
    fn empty_channel_and_payload() {
        let m = Message::new("", Vec::<u8>::new());
        assert_eq!(&*m.channel, "");
        assert!(m.payload.is_empty());

        let m_static = Message::from_static("", b"");
        assert_eq!(&*m_static.channel, "");
        assert!(m_static.payload.is_empty());
    }

    /// Проверяет создание из среза и `Bytes`, сравнивая полезную нагрузку.
    #[test]
    fn new_from_slice_and_bytes_clone() {
        // slice имеет тип &[u8; 10]
        let slice: &'static [u8; 10] = b"slice_data";

        let m1 = Message::new("chan1", &slice[..]);

        let bytes = Bytes::from_static(b"bytes_data");
        let m2 = Message::new("chan2", bytes.clone());

        assert_eq!(m1.payload, Bytes::from_static(&slice[..]));
        assert_eq!(m2.payload, bytes);
    }

    /// Проверяет создание из вектора и статического среза, сравнивая полезную нагрузку с ожидаемой.
    #[test]
    fn new_from_vec_and_static() {
        let v = vec![9u8; 10];
        let s: &[u8] = &[1, 2, 3];
        let m1 = Message::new("v", v.clone());
        let m2 = Message::new("s", s);
        assert_eq!(m1.payload, Bytes::from(v));
        assert_eq!(m2.payload, Bytes::from_static(s));
    }

    /// Проверяет, что два сообщения с одинаковыми каналами и полезной нагрузкой равны.
    #[test]
    fn message_equality() {
        let a = Message::new("a", b"x".to_vec());
        let b = Message::new("a", b"x".to_vec());
        assert_eq!(a, b);
    }

    /// Проверяет, что `Debug` вывод содержит канал и полезную нагрузку.
    #[test]
    fn debug_contains_channel_and_payload() {
        let m = Message::new("dbg", b"z".to_vec());
        let s = format!("{m:?}");
        assert!(s.contains("channel"));
        assert!(s.contains("payload"));
        assert!(s.contains("dbg"));
    }

    /// Проверяет, что клонирование большого payload не копирует данные (zero-copy).
    #[test]
    fn large_payload_clone_zero_copy() {
        let big = vec![0u8; 1_000_000];
        let m1 = Message::new("big", big.clone());
        let ptr1 = m1.payload.as_ptr();
        let m2 = m1.clone();
        assert_eq!(m2.payload.as_ptr(), ptr1);
        assert_eq!(m2.payload.len(), big.len());
    }

    /// Проверяет, что создание из `Arc<str>` сохраняет указатель.
    #[test]
    fn new_from_arc_str_retains_pointer() {
        let arc: Arc<str> = Arc::from("mychan");
        let m = Message::new(arc.clone(), b"p".to_vec());
        assert_eq!(&*arc, &*m.channel);
    }

    /// Проверяет, что вызовы `from_static` с одинаковыми именами каналов
    /// возвращают один и тот же `Arc<str>`, несмотря на разные полезные нагрузки.
    #[test]
    fn static_messages_share_pointer() {
        let m1 = Message::from_static("stat", b"1");
        let m2 = Message::from_static("stat", b"2");
        assert!(
            Arc::ptr_eq(&m1.channel, &m2.channel),
            "Identical static channels should intern to the same Arc"
        );
    }

    /// Проверяет, что `Message::new` и `Message::from_static` с одинаковыми именами
    /// используют один и тот же интернированный `Arc<str>` для канала.
    #[test]
    fn new_and_from_static_share_pointer() {
        let m1 = Message::new("mix", b"kin".to_vec());
        let m2 = Message::from_static("mix", b"dza");
        assert!(
            Arc::ptr_eq(&m1.channel, &m2.channel),
            "new and from_static with the same name should return the same Arc"
        );
    }
}
