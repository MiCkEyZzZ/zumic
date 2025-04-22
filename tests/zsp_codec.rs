use std::io::Cursor;

use zumic::network::zsp::frame::{decoder::ZSPDecoder, encoder::ZSPEncoder, zsp_types::ZSPFrame};

#[test]
fn test_roundtrip_inline_string() {
    let original = ZSPFrame::InlineString("hello".into());
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_binary_string() {
    let original = ZSPFrame::BinaryString(Some(b"world".to_vec()));
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
        ZSPFrame::InlineString("nested".into()),
        ZSPFrame::BinaryString(None),
    ]));
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_dictionary() {
    let mut items = std::collections::HashMap::new();
    items.insert(
        "key1".to_string(),
        ZSPFrame::InlineString("value1".to_string()),
    );
    items.insert("key2".to_string(), ZSPFrame::Integer(100));
    let original = ZSPFrame::Dictionary(Some(items));
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_empty_dictionary() {
    let original = ZSPFrame::Dictionary(None);
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}

// Изменили тест: теперь ожидаем Ok(None), если словарь неполный
#[test]
fn test_roundtrip_incomplete_dictionary() {
    let mut decoder = ZSPDecoder::new();
    let data = b"%2\r\n+key1\r\n+value1\r\n".to_vec(); // Недостаточно данных для второго элемента
    let mut cursor = Cursor::new(data.as_slice());
    let result = decoder.decode(&mut cursor);
    assert!(matches!(result, Ok(None))); // Ожидаем Ok(None)
}

#[test]
fn test_roundtrip_mixed_types() {
    let original = ZSPFrame::Array(Some(vec![
        ZSPFrame::InlineString("hello".into()),
        ZSPFrame::BinaryString(Some(b"world".to_vec())),
        ZSPFrame::Integer(100),
        ZSPFrame::Dictionary(Some(std::collections::HashMap::from([(
            "key1".to_string(),
            ZSPFrame::InlineString("value1".to_string()),
        )]))),
    ]));
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(encoded.as_slice());
    let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_invalid_data() {
    let data = b"Invalid data that should fail decoding".to_vec();
    let mut decoder = ZSPDecoder::new();
    let mut cursor = Cursor::new(data.as_slice());
    let result = decoder.decode(&mut cursor);

    assert!(result.is_err()); // Ожидаем ошибку, так как данные некорректны.
}
