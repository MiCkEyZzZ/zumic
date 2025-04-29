use std::sync::Arc;

use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub channel: Arc<str>,
    pub payload: Bytes,
}

impl Message {
    /// Создаёт сообщение из любого `String` или `&str` и `Bytes`/`Vec<u8>`.
    pub fn new<S>(channel: S, payload: impl Into<Bytes>) -> Self
    where
        S: Into<Arc<str>>,
    {
        Self {
            channel: channel.into(),
            payload: payload.into(),
        }
    }

    /// Быстрый путь для полностью статичных сообщений (zero-copy).
    pub fn from_static(channel: &'static str, payload: &'static [u8]) -> Self {
        Self {
            channel: Arc::from(channel),
            payload: Bytes::from_static(payload),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_and_vec() {
        let ch = "news";
        let pl_vec = vec![1, 2, 3];
        let msg: Message = Message::new(ch, pl_vec.clone());
        assert_eq!(&*msg.channel, "news");
        assert_eq!(msg.payload, Bytes::from(pl_vec));
    }

    #[test]
    fn new_from_string_and_bytes() {
        let ch_string = String::from("updates");
        let pl_bytes = Bytes::from_static(b"hello");
        let msg: Message = Message::new(ch_string.clone(), pl_bytes.clone());
        assert_eq!(&*msg.channel, &ch_string);
        assert_eq!(msg.payload, pl_bytes);
    }

    #[test]
    fn clone_preserves_arc_and_bytes_zero_copy() {
        let msg1 = Message::new("chan", Bytes::from_static(b"x"));
        let arc_ptr = Arc::as_ptr(&msg1.channel);
        let bytes_ptr = msg1.payload.as_ptr();

        let msg2 = msg1.clone();
        // Проверяем, что ptr канала совпадает с оригиналом
        assert_eq!(Arc::as_ptr(&msg2.channel), arc_ptr);
        // Проверяем, что Bytes тоже не копирует данные
        assert_eq!(msg2.payload.as_ptr(), bytes_ptr);
        assert_eq!(msg1.payload, msg2.payload);
        assert_eq!(&*msg2.channel, "chan");
    }

    #[test]
    fn from_static_zero_copy() {
        let msg = Message::from_static("static_chan", b"data");
        // channel is &'static str inside Arc
        assert_eq!(&*msg.channel, "static_chan");
        // payload is from_static
        assert_eq!(msg.payload, Bytes::from_static(b"data"));
    }

    #[test]
    fn mix_new_and_from_static() {
        let m1 = Message::new("kin", b"dzadza".to_vec());
        let m2 = Message::from_static("kin", b"dzadza");
        // channel strings equal
        assert_eq!(&*m1.channel, &*m2.channel);
        // payloads equal
        assert_eq!(m1.payload, m2.payload);
        // but Arc pointers likely differ
        assert!(!Arc::ptr_eq(&m1.channel, &m2.channel));
    }

    #[test]
    fn empty_channel_and_payload() {
        let m = Message::new("", Vec::<u8>::new());
        assert_eq!(&*m.channel, "");
        assert!(m.payload.is_empty());

        let m_static = Message::from_static("", b"");
        assert_eq!(&*m_static.channel, "");
        assert!(m_static.payload.is_empty());
    }

    #[test]
    fn new_from_slice_and_bytes_clone() {
        let slice = b"slice_data";
        let bytes = Bytes::from_static(b"bytes_data");

        // &[u8] → Bytes через impl From<&[u8]>
        let m1 = Message::new("chan1", slice as &[u8]);

        // для Bytes нужно передавать сам объект или его клон
        let m2 = Message::new("chan2", bytes.clone());

        assert_eq!(m1.payload, Bytes::from_static(slice));
        assert_eq!(m2.payload, bytes);
    }

    #[test]
    fn new_from_vec_and_static() {
        let v = vec![9u8; 10];
        let s: &[u8] = &[1, 2, 3];
        let m1 = Message::new("v", v.clone());
        let m2 = Message::new("s", s);
        assert_eq!(m1.payload, Bytes::from(v));
        assert_eq!(m2.payload, Bytes::from_static(s));
    }

    #[test]
    fn message_equality() {
        let a = Message::new("a", b"x".to_vec());
        let b = Message::new("a", b"x".to_vec());
        assert_eq!(a, b);
    }

    #[test]
    fn debug_contains_channel_and_payload() {
        let m = Message::new("dbg", b"z".to_vec());
        let s = format!("{:?}", m);
        assert!(s.contains("channel"));
        assert!(s.contains("payload"));
        assert!(s.contains("dbg"));
    }

    #[test]
    fn large_payload_clone_zero_copy() {
        let big = vec![0u8; 1_000_000];
        let m1 = Message::new("big", big.clone());
        let ptr1 = m1.payload.as_ptr();
        let m2 = m1.clone();
        assert_eq!(m2.payload.as_ptr(), ptr1);
        assert_eq!(m2.payload.len(), big.len());
    }

    #[test]
    fn new_from_arc_str_retains_pointer() {
        let arc: Arc<str> = Arc::from("mychan");
        let m = Message::new(arc.clone(), b"p".to_vec());
        assert!(Arc::ptr_eq(&arc, &m.channel));
    }
}
