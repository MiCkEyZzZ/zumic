use std::sync::Arc;

use bytes::Bytes;

use super::intern_channel;

/// Represents a single message in the pub/sub system.
///
/// Contains:
/// - The channel name through which the message was sent;
/// - The message payload as bytes.
///
/// Used for both standard subscriptions (`SUBSCRIBE`) and pattern-based subscriptions (`PSUBSCRIBE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Channel to which the message was published.
    pub channel: Arc<str>,
    /// Message content.
    pub payload: Bytes,
}

impl Message {
    /// Creates a new message from a channel and payload.
    ///
    /// The channel can be any type implementing `AsRef<str>` (`&str`, `String`, `Arc<str>`),
    /// and the payload can be any type convertible into `Bytes` (`Vec<u8>`, `&[u8]`, `Bytes`).
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

    /// Creates a message from fully static data without copying.
    ///
    /// This is the fastest way to create a message if both the channel
    /// and content are `'static`.
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

    /// Verifies creation of a message from a string and vector: correct channel
    /// and conversion of payload to `Bytes`.
    #[test]
    fn test_from_and_vec() {
        let ch = "news";
        let pl_vec = vec![1, 2, 3];
        let msg: Message = Message::new(ch, pl_vec.clone());
        assert_eq!(&*msg.channel, "news");
        assert_eq!(msg.payload, Bytes::from(pl_vec));
    }

    /// Verifies creation from `String` and `Bytes`, including matching references and contents.
    #[test]
    fn new_from_string_and_bytes() {
        let ch_string = String::from("updates");
        let pl_bytes = Bytes::from_static(b"hello");
        let msg: Message = Message::new(ch_string.clone(), pl_bytes.clone());
        assert_eq!(&*msg.channel, &ch_string);
        assert_eq!(msg.payload, pl_bytes);
    }

    /// Verifies that `clone` preserves pointers (Arc and Bytes) without copying.
    #[test]
    fn clone_preserves_arc_and_bytes_zero_copy() {
        let msg1 = Message::new("chan", Bytes::from_static(b"x"));
        let arc_ptr = Arc::as_ptr(&msg1.channel);
        let bytes_ptr = msg1.payload.as_ptr();

        let msg2 = msg1.clone();
        assert_eq!(Arc::as_ptr(&msg2.channel), arc_ptr);
        assert_eq!(msg2.payload.as_ptr(), bytes_ptr);
    }

    /// Verifies creation from static data without copying (`from_static`).
    #[test]
    fn from_static_zero_copy() {
        let msg = Message::from_static("static_chan", b"data");
        assert_eq!(&*msg.channel, "static_chan");
        assert_eq!(msg.payload, Bytes::from_static(b"data"));
    }

    /// Compares behavior of `new` and `from_static`: channels equal by value,
    /// and pointers equal due to interning.
    #[test]
    fn mix_new_and_from_static() {
        let m1 = Message::new("kin", b"dzadza".to_vec());
        let m2 = Message::from_static("kin", b"dzadza");
        assert_eq!(&*m1.channel, &*m2.channel);
        assert_eq!(m1.payload, m2.payload);
        assert!(Arc::ptr_eq(&m1.channel, &m2.channel));
    }

    /// Verifies correct handling of empty channel and payload (new and from_static).
    #[test]
    fn empty_channel_and_payload() {
        let m = Message::new("", Vec::<u8>::new());
        assert_eq!(&*m.channel, "");
        assert!(m.payload.is_empty());

        let m_static = Message::from_static("", b"");
        assert_eq!(&*m_static.channel, "");
        assert!(m_static.payload.is_empty());
    }

    /// Verifies creation from slice and `Bytes`, comparing payload.
    #[test]
    fn new_from_slice_and_bytes_clone() {
        let slice = b"slice_data";
        let bytes = Bytes::from_static(b"bytes_data");

        let m1 = Message::new("chan1", slice as &[u8]);
        let m2 = Message::new("chan2", bytes.clone());

        assert_eq!(m1.payload, Bytes::from_static(slice));
        assert_eq!(m2.payload, bytes);
    }

    /// Verifies creation from vector and static slice, comparing payload with expected.
    #[test]
    fn new_from_vec_and_static() {
        let v = vec![9u8; 10];
        let s: &[u8] = &[1, 2, 3];
        let m1 = Message::new("v", v.clone());
        let m2 = Message::new("s", s);
        assert_eq!(m1.payload, Bytes::from(v));
        assert_eq!(m2.payload, Bytes::from_static(s));
    }

    /// Verifies that two messages with identical channels and payloads are equal.
    #[test]
    fn message_equality() {
        let a = Message::new("a", b"x".to_vec());
        let b = Message::new("a", b"x".to_vec());
        assert_eq!(a, b);
    }

    /// Checks that `Debug` output contains channel and payload.
    #[test]
    fn debug_contains_channel_and_payload() {
        let m = Message::new("dbg", b"z".to_vec());
        let s = format!("{m:?}");
        assert!(s.contains("channel"));
        assert!(s.contains("payload"));
        assert!(s.contains("dbg"));
    }

    /// Verifies that cloning a large payload does not copy data (zero-copy).
    #[test]
    fn large_payload_clone_zero_copy() {
        let big = vec![0u8; 1_000_000];
        let m1 = Message::new("big", big.clone());
        let ptr1 = m1.payload.as_ptr();
        let m2 = m1.clone();
        assert_eq!(m2.payload.as_ptr(), ptr1);
        assert_eq!(m2.payload.len(), big.len());
    }

    /// Verifies that creating from an `Arc<str>` retains the pointer.
    #[test]
    fn new_from_arc_str_retains_pointer() {
        let arc: Arc<str> = Arc::from("mychan");
        let m = Message::new(arc.clone(), b"p".to_vec());
        assert_eq!(&*arc, &*m.channel);
    }

    /// Verifies that calls to `from_static` with the same channel name
    /// return the same `Arc<str>`, despite different payloads.
    #[test]
    fn static_messages_share_pointer() {
        let m1 = Message::from_static("stat", b"1");
        let m2 = Message::from_static("stat", b"2");
        assert!(
            Arc::ptr_eq(&m1.channel, &m2.channel),
            "Identical static channels should intern to the same Arc"
        );
    }

    /// Verifies that `Message::new` and `Message::from_static` with the same name
    /// use the same interned `Arc<str>` for the channel.
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
