use std::collections::HashMap;

use zumic::zsp::frame::{decoder::ZSPDecoder, encoder::ZSPEncoder, zsp_types::ZSPFrame};

#[test]
fn test_roundtrip_inline_string() {
    let original = ZSPFrame::InlineString("hello".into());
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_binary_string() {
    let original = ZSPFrame::BinaryString(Some(b"world".to_vec()));
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_array() {
    let original = ZSPFrame::Array(vec![
        ZSPFrame::Integer(42),
        ZSPFrame::InlineString("nested".into()),
        ZSPFrame::BinaryString(None),
    ]);
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_dictionary() {
    let mut items = std::collections::HashMap::new();
    items.insert("key1".into(), ZSPFrame::InlineString("value1".into()));
    items.insert("key2".into(), ZSPFrame::Integer(100));
    let original = ZSPFrame::Dictionary(items);
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_empty_dictionary() {
    let original = ZSPFrame::Dictionary(HashMap::new());
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();

    assert_eq!(original, decoded);
}

// Изменили тест: теперь ожидаем Ok(None), если словарь неполный
#[test]
fn test_roundtrip_incomplete_dictionary() {
    let mut decoder = ZSPDecoder::new();
    let data = b"%2\r\n+key1\r\n+value1\r\n".to_vec(); // Недостаточно данных для второго элемента
    let mut slice = data.as_slice();
    let result = decoder.decode(&mut slice);
    assert!(matches!(result, Ok(None))); // Ожидаем Ok(None)
}

#[test]
fn test_roundtrip_mixed_types() {
    let original = ZSPFrame::Array(vec![
        ZSPFrame::InlineString("hello".into()),
        ZSPFrame::BinaryString(Some(b"world".to_vec())),
        ZSPFrame::Integer(100),
        ZSPFrame::Dictionary(HashMap::from([(
            "key1".into(),
            ZSPFrame::InlineString("value1".into()),
        )])),
    ]);
    let encoded = ZSPEncoder::encode(&original).unwrap();

    let mut decoder = ZSPDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_roundtrip_invalid_data() {
    let data = b"Invalid data that should fail decoding".to_vec();
    let mut decoder = ZSPDecoder::new();
    let mut slice = data.as_slice();
    let result = decoder.decode(&mut slice);

    assert!(result.is_err()); // Ожидаем ошибку, так как данные некорректны.
}
