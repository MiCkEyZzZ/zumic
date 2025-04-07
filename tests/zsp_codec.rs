use std::io::Cursor;

use zumic::network::zsp::{decoder::ZSPDecoder, encoder::ZSPEncoder, types::ZSPFrame};

#[test]
fn test_roundtrip_simple_string() {
    let original = ZSPFrame::SimpleString("hello".into());
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_bulk_string() {
    let original = ZSPFrame::BulkString(Some(b"world".to_vec()));
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_array() {
    let original = ZSPFrame::Array(Some(vec![
        ZSPFrame::Integer(42),
        ZSPFrame::SimpleString("nested".into()),
        ZSPFrame::BulkString(None),
    ]));
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}
