use std::collections::HashMap;

use zumic::zsp::{decoder::ZspDecoder, encoder::ZspEncoder, zsp_types::ZspFrame};

/// Тест проверяет, что `InlineString` корректно кодируется и
/// декодируется обратно без потерь.
#[test]
fn test_roundtrip_inline_string() {
    let original = ZspFrame::InlineString("hello".into());
    let encoded = ZspEncoder::encode(&original).unwrap();
    let mut decoder = ZspDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();
    assert_eq!(original, decoded);
}

/// Тест проверяет, что `BinaryString` с содержимым корректно проходит
/// процесс кодирования и декодирования.
#[test]
fn test_roundtrip_binary_string() {
    let original = ZspFrame::BinaryString(Some(b"world".to_vec()));
    let encoded = ZspEncoder::encode(&original).unwrap();
    let mut decoder = ZspDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();
    assert_eq!(original, decoded);
}

/// Тест проверяет кодирование и декодирование массива, содержащего
/// разные типы фреймов.
#[test]
fn test_roundtrip_array() {
    let original = ZspFrame::Array(vec![
        ZspFrame::Integer(42),
        ZspFrame::InlineString("nested".into()),
        ZspFrame::BinaryString(None),
    ]);
    let encoded = ZspEncoder::encode(&original).unwrap();
    let mut decoder = ZspDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();
    assert_eq!(original, decoded);
}

/// Тест проверяет кодирование и декодирование словаря с несколькими
/// ключами и значениями разных типов.
#[test]
fn test_roundtrip_dictionary() {
    let mut items = std::collections::HashMap::new();
    items.insert("key1".into(), ZspFrame::InlineString("value1".into()));
    items.insert("key2".into(), ZspFrame::Integer(100));
    let original = ZspFrame::Dictionary(items);
    let encoded = ZspEncoder::encode(&original).unwrap();
    let mut decoder = ZspDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();
    assert_eq!(original, decoded);
}

/// Тест проверяет обработку пустого словаря — должен кодироваться и
/// декодироваться корректно.
#[test]
fn test_roundtrip_empty_dictionary() {
    let original = ZspFrame::Dictionary(HashMap::new());
    let encoded = ZspEncoder::encode(&original).unwrap();

    let mut decoder = ZspDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();
    assert_eq!(original, decoded);
}

/// Тест проверяет поведение при неполном словаре — ожидается `Ok(None)`
/// (недостаточно данных для разбора).
#[test]
fn test_roundtrip_incomplete_dictionary() {
    let mut decoder = ZspDecoder::new();
    let data = b"%2\r\n+key1\r\n+value1\r\n".to_vec(); // Недостаточно данных для второго элемента
    let mut slice = data.as_slice();
    let result = decoder.decode(&mut slice);
    assert!(matches!(result, Ok(None))); // Ожидаем Ok(None)
}

/// Тест проверяет, что структура с вложенными фреймами разных типов
/// кодируется и декодируется без потерь.
#[test]
fn test_roundtrip_mixed_types() {
    let original = ZspFrame::Array(vec![
        ZspFrame::InlineString("hello".into()),
        ZspFrame::BinaryString(Some(b"world".to_vec())),
        ZspFrame::Integer(100),
        ZspFrame::Dictionary(HashMap::from([(
            "key1".into(),
            ZspFrame::InlineString("value1".into()),
        )])),
    ]);
    let encoded = ZspEncoder::encode(&original).unwrap();
    let mut decoder = ZspDecoder::new();
    let mut slice = encoded.as_slice();
    let decoded = decoder.decode(&mut slice).unwrap().unwrap();
    assert_eq!(original, decoded);
}

/// Тест проверяет, что при передаче некорректных данных декодер
/// возвращает ошибку.
#[test]
fn test_roundtrip_invalid_data() {
    let data = b"Invalid data that should fail decoding".to_vec();
    let mut decoder = ZspDecoder::new();
    let mut slice = data.as_slice();
    let result = decoder.decode(&mut slice);
    assert!(result.is_err()); // Ожидаем ошибку, так как данные некорректны.
}
