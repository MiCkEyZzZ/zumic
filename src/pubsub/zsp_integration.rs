use bytes::Bytes;
use std::borrow::Cow;

use crate::{
    network::zsp::{
        frame::{ZspDecoder, ZspEncoder, ZspFrame},
        protocol::command::{Command as ZspCommand, PubSubMessage},
    },
    pubsub::{Message, MessageMetadata, MessagePayload},
    RecvError,
};

/// Ошибки интеграции ZSP
#[derive(Debug)]
pub enum ZspIntegrationError {
    Encode(crate::EncodeError),
    Decode(crate::DecodeError),
    Serialization(String),
}

/// Тип для команд ZSP (совместимый с вашим протоколом)
#[derive(Debug, Clone)]
pub enum PubSubCommand {
    Subscribe(Vec<String>),
    Unsubscribe(Vec<String>),
    Publish(String, MessagePayload),
}

/// Кодирует Message в байты, используя ваш ZspEncoder
pub fn encode_message(msg: &Message) -> Result<Bytes, RecvError> {
    let zsp_frame = message_to_zsp_frame(msg);
    let encoded = ZspEncoder::encode(&zsp_frame)?;
    Ok(Bytes::from(encoded))
}

/// Декодирует Message из байтов, используя ваш ZspDecoder
pub fn decode_message(data: &mut Bytes) -> Result<Option<Message>, RecvError> {
    if data.is_empty() {
        return Ok(None);
    }

    let mut decoder = ZspDecoder::new();

    // Клонируем Bytes (дёшево, это Arc на буфер)
    let slice = data.clone();

    if let Some(frame) = decoder.decode(&mut slice.as_ref())? {
        let consumed = data.len() - slice.len();
        *data = data.slice(consumed..);
        let message = zsp_frame_to_message(frame)?;
        Ok(Some(message))
    } else {
        Ok(None)
    }
}

/// Кодирует SUBSCRIBE команду
pub fn encode_subscribe(channels: &[String]) -> Result<Bytes, RecvError> {
    let mut components = vec![ZspFrame::InlineString(Cow::Borrowed("SUBSCRIBE"))];

    for channel in channels {
        components.push(ZspFrame::BinaryString(Some(channel.as_bytes().to_vec())));
    }

    let frame = ZspFrame::Array(components);
    let encoded = ZspEncoder::encode(&frame)?;
    Ok(Bytes::from(encoded))
}

/// Кодирует UNSUBSCRIBE команду
pub fn encode_unsubscribe(channels: &[String]) -> Result<Bytes, RecvError> {
    let mut components = vec![ZspFrame::InlineString(Cow::Borrowed("UNSUBSCRIBE"))];

    for channel in channels {
        components.push(ZspFrame::BinaryString(Some(channel.as_bytes().to_vec())));
    }

    let frame = ZspFrame::Array(components);
    let encoded = ZspEncoder::encode(&frame)?;
    Ok(Bytes::from(encoded))
}

/// Кодирует PUBLISH команду
pub fn encode_publish(
    channel: &str,
    payload: &MessagePayload,
) -> Result<Bytes, RecvError> {
    let pubsub_message = message_payload_to_pubsub(payload);
    let _cmd = ZspCommand::Publish {
        channel: channel.to_string(),
        message: pubsub_message,
    };

    // Здесь вам нужно будет добавить функцию command_to_frame в ваш ZSP
    // Пока что делаем вручную:
    let mut components = vec![
        ZspFrame::InlineString(Cow::Borrowed("PUBLISH")),
        ZspFrame::BinaryString(Some(channel.as_bytes().to_vec())),
    ];

    match payload {
        MessagePayload::Bytes(bytes) => {
            components.push(ZspFrame::InlineString(Cow::Borrowed("BYTES")));
            components.push(ZspFrame::BinaryString(Some(bytes.to_vec())));
        }
        MessagePayload::String(s) => {
            components.push(ZspFrame::InlineString(Cow::Borrowed("STRING")));
            components.push(ZspFrame::BinaryString(Some(s.as_bytes().to_vec())));
        }
        MessagePayload::Json(json) => {
            let json_str = serde_json::to_string(json)
                .map_err(|e| RecvError::SerializationError(e.to_string()))?;
            components.push(ZspFrame::InlineString(Cow::Borrowed("JSON")));
            components.push(ZspFrame::BinaryString(Some(json_str.into_bytes())));
        }
        MessagePayload::Serialized { data, content_type } => {
            components.push(ZspFrame::InlineString(Cow::Borrowed("SERIALIZED")));
            components.push(ZspFrame::BinaryString(Some(
                content_type.as_bytes().to_vec(),
            )));
            components.push(ZspFrame::BinaryString(Some(data.to_vec())));
        }
    }

    let frame = ZspFrame::Array(components);
    let encoded = ZspEncoder::encode(&frame)?;
    Ok(Bytes::from(encoded))
}

/// Декодирует команду (SUBSCRIBE, UNSUBSCRIBE, PUBLISH)
pub fn decode_command(data: &mut Bytes) -> Result<Option<PubSubCommand>, RecvError> {
    if data.is_empty() {
        return Ok(None);
    }

    // Декодируем фрейм и сразу преобразуем в owned PubSubCommand внутри блока.
    let (cmd_opt, consumed) = {
        let mut slice: &[u8] = &data[..];
        let mut decoder = ZspDecoder::new();

        match decoder.decode(&mut slice)? {
            Some(ZspFrame::Array(items)) if !items.is_empty() => {
                let consumed = data.len() - slice.len();

                // Разбираем команду здесь, возвращая owned значения.
                let cmd_str = match &items[0] {
                    ZspFrame::InlineString(s) => s.as_ref().to_uppercase(),
                    ZspFrame::BinaryString(Some(bytes)) => {
                        String::from_utf8_lossy(bytes).to_uppercase()
                    }
                    _ => {
                        return Err(RecvError::SerializationError(
                            "Invalid command format".to_string(),
                        ))
                    }
                };

                let command = match cmd_str.as_str() {
                    "SUBSCRIBE" => {
                        let channels = items[1..]
                            .iter()
                            .map(|frame| extract_binary_string(frame))
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .map(|b| String::from_utf8_lossy(&b).to_string())
                            .collect();
                        PubSubCommand::Subscribe(channels)
                    }
                    "UNSUBSCRIBE" => {
                        let channels = items[1..]
                            .iter()
                            .map(|frame| extract_binary_string(frame))
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .map(|b| String::from_utf8_lossy(&b).to_string())
                            .collect();
                        PubSubCommand::Unsubscribe(channels)
                    }
                    "PUBLISH" => {
                        if items.len() < 3 {
                            return Err(RecvError::SerializationError(
                                "PUBLISH requires channel and payload".to_string(),
                            ));
                        }

                        let channel_bytes = extract_binary_string(&items[1])?;
                        let channel = String::from_utf8_lossy(&channel_bytes).to_string();

                        // Собираем PubSubMessage (owned), затем конвертируем в MessagePayload
                        let pubsub_msg = if items.len() == 3 {
                            // legacy: assume bytes
                            let d = extract_binary_string(&items[2])?;
                            PubSubMessage::Bytes(d)
                        } else {
                            let payload_type = match &items[2] {
                                ZspFrame::InlineString(s) => s.as_ref(),
                                _ => {
                                    return Err(RecvError::SerializationError(
                                        "Payload type must be string".to_string(),
                                    ))
                                }
                            };

                            match payload_type.to_uppercase().as_str() {
                                "BYTES" => {
                                    let d = extract_binary_string(&items[3])?;
                                    PubSubMessage::Bytes(d)
                                }
                                "STRING" => {
                                    let d = extract_binary_string(&items[3])?;
                                    let s = String::from_utf8(d).map_err(|e| {
                                        RecvError::SerializationError(e.to_string())
                                    })?;
                                    PubSubMessage::String(s)
                                }
                                "JSON" => {
                                    let d = extract_binary_string(&items[3])?;
                                    let s = String::from_utf8(d).map_err(|e| {
                                        RecvError::SerializationError(e.to_string())
                                    })?;
                                    let v = serde_json::from_str(&s).map_err(|e| {
                                        RecvError::SerializationError(e.to_string())
                                    })?;
                                    PubSubMessage::Json(v)
                                }
                                "SERIALIZED" => {
                                    if items.len() < 5 {
                                        return Err(RecvError::SerializationError(
                                            "SERIALIZED requires content_type and data".to_string(),
                                        ));
                                    }
                                    let ct = extract_binary_string(&items[3])?;
                                    let content_type = String::from_utf8_lossy(&ct).to_string();
                                    let d = extract_binary_string(&items[4])?;
                                    PubSubMessage::Serialized {
                                        data: d,
                                        content_type,
                                    }
                                }
                                other => {
                                    return Err(RecvError::SerializationError(format!(
                                        "Unknown payload type: {other}"
                                    )))
                                }
                            }
                        };

                        // Конвертируем централизованно в MessagePayload
                        let payload = pubsub_to_message_payload(pubsub_msg);

                        PubSubCommand::Publish(channel, payload)
                    }
                    other => {
                        return Err(RecvError::SerializationError(format!(
                            "Unknown command: {other}"
                        )))
                    }
                };

                (Some(command), consumed)
            }
            Some(_) => {
                return Err(RecvError::SerializationError(
                    "Expected array command".to_string(),
                ))
            }
            None => (None, 0),
        }
    }; // slice и decoder умирают здесь

    if let Some(cmd) = cmd_opt {
        *data = data.slice(consumed..);
        return Ok(Some(cmd));
    }

    Ok(None)
}

impl From<crate::DecodeError> for RecvError {
    fn from(err: crate::DecodeError) -> Self {
        RecvError::from(ZspIntegrationError::Decode(err))
    }
}

impl From<crate::EncodeError> for RecvError {
    fn from(err: crate::EncodeError) -> Self {
        RecvError::from(ZspIntegrationError::Encode(err))
    }
}

impl From<ZspIntegrationError> for RecvError {
    fn from(err: ZspIntegrationError) -> Self {
        match err {
            ZspIntegrationError::Encode(e) => {
                RecvError::SerializationError(format!("Encode: {e:?}"))
            }
            ZspIntegrationError::Decode(e) => {
                RecvError::SerializationError(format!("Decode: {e:?}"))
            }
            ZspIntegrationError::Serialization(s) => RecvError::SerializationError(s),
        }
    }
}

// Вспомогательные небуличные ф-ии

/// Преобразует Message в ZspFrame для кодирования
fn message_to_zsp_frame(msg: &Message) -> ZspFrame<'_> {
    let mut components = Vec::new();

    // MESSAGE marker
    components.push(ZspFrame::InlineString(Cow::Borrowed("MESSAGE")));

    // Channel
    components.push(ZspFrame::BinaryString(Some(
        msg.channel.as_bytes().to_vec(),
    )));

    // Payload type + data
    match &msg.payload {
        MessagePayload::Bytes(bytes) => {
            components.push(ZspFrame::InlineString(Cow::Borrowed("BYTES")));
            components.push(ZspFrame::BinaryString(Some(bytes.to_vec())));
        }
        MessagePayload::String(s) => {
            components.push(ZspFrame::InlineString(Cow::Borrowed("STRING")));
            components.push(ZspFrame::BinaryString(Some(s.as_bytes().to_vec())));
        }
        MessagePayload::Json(json) => {
            let json_str = serde_json::to_string(json).unwrap_or_default();
            components.push(ZspFrame::InlineString(Cow::Borrowed("JSON")));
            components.push(ZspFrame::BinaryString(Some(json_str.into_bytes())));
        }
        MessagePayload::Serialized { data, content_type } => {
            components.push(ZspFrame::InlineString(Cow::Borrowed("SERIALIZED")));
            components.push(ZspFrame::BinaryString(Some(
                content_type.as_bytes().to_vec(),
            )));
            components.push(ZspFrame::BinaryString(Some(data.to_vec())));
        }
    }

    // Optional metadata
    if let Some(metadata) = &msg.metadata {
        if let Ok(metadata_json) = serde_json::to_string(metadata) {
            components.push(ZspFrame::InlineString(Cow::Borrowed("METADATA")));
            components.push(ZspFrame::BinaryString(Some(metadata_json.into_bytes())));
        }
    }

    ZspFrame::Array(components)
}

/// Преобразует ZspFrame обратно в Message
fn zsp_frame_to_message(frame: ZspFrame) -> Result<Message, ZspIntegrationError> {
    match frame {
        ZspFrame::Array(components) if components.len() >= 4 => {
            // Проверяем MESSAGE marker
            if let ZspFrame::InlineString(msg_type) = &components[0] {
                if msg_type.as_ref() != "MESSAGE" {
                    return Err(ZspIntegrationError::Serialization(
                        "Expected MESSAGE marker".to_string(),
                    ));
                }
            } else {
                return Err(ZspIntegrationError::Serialization(
                    "Invalid message format".to_string(),
                ));
            }

            // Извлекаем канал
            let channel = extract_binary_string(&components[1])?;
            let channel_str = String::from_utf8(channel)
                .map_err(|e| ZspIntegrationError::Serialization(e.to_string()))?;

            // Извлекаем тип payload
            let payload_type = if let ZspFrame::InlineString(pt) = &components[2] {
                pt.as_ref().to_string()
            } else {
                return Err(ZspIntegrationError::Serialization(
                    "Payload type must be inline string".to_string(),
                ));
            };

            // Создаём payload
            let payload = match payload_type.as_str() {
                "BYTES" => {
                    let data = extract_binary_string(&components[3])?;
                    MessagePayload::Bytes(Bytes::from(data))
                }
                "STRING" => {
                    let data = extract_binary_string(&components[3])?;
                    let string = String::from_utf8(data)
                        .map_err(|e| ZspIntegrationError::Serialization(e.to_string()))?;
                    MessagePayload::String(string)
                }
                "JSON" => {
                    let data = extract_binary_string(&components[3])?;
                    let json_str = String::from_utf8(data)
                        .map_err(|e| ZspIntegrationError::Serialization(e.to_string()))?;
                    let json_value = serde_json::from_str(&json_str)
                        .map_err(|e| ZspIntegrationError::Serialization(e.to_string()))?;
                    MessagePayload::Json(json_value)
                }
                "SERIALIZED" => {
                    if components.len() < 5 {
                        return Err(ZspIntegrationError::Serialization(
                            "SERIALIZED requires content type and data".to_string(),
                        ));
                    }

                    // Получаем байты content_type и данные
                    let content_type_bytes = extract_binary_string(&components[3])?;
                    let data_bytes = extract_binary_string(&components[4])?;

                    // Преобразуем в String без санитизации
                    let content_type_str = String::from_utf8(content_type_bytes)
                        .map_err(|e| ZspIntegrationError::Serialization(e.to_string()))?;

                    MessagePayload::Serialized {
                        data: Bytes::from(data_bytes),
                        content_type: content_type_str,
                    }
                }
                _ => {
                    return Err(ZspIntegrationError::Serialization(format!(
                        "Unknown payload type: {payload_type}"
                    )))
                }
            };

            let mut message = Message::with_payload(channel_str, payload);

            // Проверяем метаданные
            let metadata_start = match payload_type.as_str() {
                "SERIALIZED" => 5,
                _ => 4,
            };

            if components.len() > metadata_start {
                if let ZspFrame::InlineString(metadata_marker) = &components[metadata_start] {
                    if metadata_marker.as_ref() == "METADATA"
                        && components.len() > metadata_start + 1
                    {
                        let metadata_data = extract_binary_string(&components[metadata_start + 1])?;
                        let metadata_str = String::from_utf8(metadata_data)
                            .map_err(|e| ZspIntegrationError::Serialization(e.to_string()))?;
                        let metadata: MessageMetadata = serde_json::from_str(&metadata_str)
                            .map_err(|e| ZspIntegrationError::Serialization(e.to_string()))?;
                        message = message.with_metadata(metadata);
                    }
                }
            }

            Ok(message)
        }
        _ => Err(ZspIntegrationError::Serialization(
            "Invalid message array".to_string(),
        )),
    }
}

/// Преобразует MessagePayload в PubSubMessage
fn message_payload_to_pubsub(payload: &MessagePayload) -> PubSubMessage {
    match payload {
        MessagePayload::Bytes(bytes) => PubSubMessage::Bytes(bytes.to_vec()),
        MessagePayload::String(s) => PubSubMessage::String(s.clone()),
        MessagePayload::Json(json) => PubSubMessage::Json(json.clone()),
        MessagePayload::Serialized { data, content_type } => PubSubMessage::Serialized {
            data: data.to_vec(),
            content_type: content_type.clone(),
        },
    }
}

/// Преобразует PubSubMessage в MessagePayload
fn pubsub_to_message_payload(message: PubSubMessage) -> MessagePayload {
    match message {
        PubSubMessage::Bytes(bytes) => MessagePayload::Bytes(Bytes::from(bytes)),
        PubSubMessage::String(s) => MessagePayload::String(s),
        PubSubMessage::Json(json) => MessagePayload::Json(json),
        PubSubMessage::Serialized { data, content_type } => MessagePayload::Serialized {
            data: Bytes::from(data),
            content_type,
        },
    }
}

/// Вспомогательная функция для извлечения binary string
fn extract_binary_string(frame: &ZspFrame) -> Result<Vec<u8>, ZspIntegrationError> {
    match frame {
        ZspFrame::BinaryString(Some(bytes)) => Ok(bytes.clone()),
        ZspFrame::InlineString(s) => Ok(s.as_bytes().to_vec()),
        _ => Err(ZspIntegrationError::Serialization(
            "Expected binary string".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_message_roundtrip() {
        let original_msg = Message::from_string("test_channel", "Hello, World!")
            .with_timestamp()
            .with_message_id("msg_123");

        let encoded = encode_message(&original_msg).unwrap();
        let mut data = encoded;
        let decoded_msg = decode_message(&mut data).unwrap().unwrap();

        assert_eq!(original_msg.channel, decoded_msg.channel);
        match (&original_msg.payload, &decoded_msg.payload) {
            (MessagePayload::String(s), MessagePayload::String(decoded)) => {
                assert_eq!(s, decoded);
            }
            _ => panic!("Payload types don't match"),
        }
        assert_eq!(original_msg.metadata, decoded_msg.metadata);
    }

    #[test]
    fn test_json_payload_roundtrip() {
        let json_data = json!({
            "user": "anton",
            "action": "login",
            "timestamp": 1234567890
        });

        let msg = Message::from_json("events", json_data.clone());
        let encoded = encode_message(&msg).unwrap();
        let mut data = encoded;
        let decoded = decode_message(&mut data).unwrap().unwrap();

        match decoded.payload {
            MessagePayload::Json(decoded_json) => {
                assert_eq!(json_data, decoded_json);
            }
            _ => panic!("Expected JSON payload"),
        }
    }

    #[test]
    fn test_serialized_payload_roundtrip() {
        let payload_bytes = Bytes::from(vec![1, 2, 3, 4, 5]);
        let msg = Message::with_payload(
            "binary_channel",
            MessagePayload::Serialized {
                data: payload_bytes.clone(),
                content_type: "application/octet-stream".to_string(),
            },
        );

        let encoded = encode_message(&msg).unwrap();
        let mut data = encoded;
        let decoded = decode_message(&mut data).unwrap().unwrap();

        match decoded.payload {
            MessagePayload::Serialized { data, content_type } => {
                assert_eq!(payload_bytes, data);
                assert_eq!("application/octet-stream", content_type);
            }
            _ => panic!("Expected Serialized payload"),
        }
    }

    #[test]
    fn test_subscribe_command_roundtrip() {
        let channels = vec!["ch1".to_string(), "ch2".to_string(), "ch3".to_string()];

        let encoded = encode_subscribe(&channels).unwrap();
        let mut data = encoded;
        let decoded_cmd = decode_command(&mut data).unwrap().unwrap();

        match decoded_cmd {
            PubSubCommand::Subscribe(decoded_channels) => {
                assert_eq!(channels, decoded_channels);
            }
            _ => panic!("Expected Subscribe command"),
        }
    }

    #[test]
    fn test_publish_command_enhanced_format() {
        let channel = "test_channel";
        let payload = MessagePayload::String("Hello, World!".to_string());

        let encoded = encode_publish(channel, &payload).unwrap();
        let mut data = encoded;
        let decoded_cmd = decode_command(&mut data).unwrap().unwrap();

        match decoded_cmd {
            PubSubCommand::Publish(decoded_channel, decoded_payload) => {
                assert_eq!(channel, decoded_channel);
                match decoded_payload {
                    MessagePayload::String(s) => assert_eq!("Hello, World!", s),
                    _ => panic!("Expected string payload"),
                }
            }
            _ => panic!("Expected Publish command"),
        }
    }

    #[test]
    fn test_publish_command_legacy_format() {
        // Имитировать устаревший формат вручную
        let components = vec![
            ZspFrame::InlineString(Cow::Borrowed("PUBLISH")),
            ZspFrame::BinaryString(Some(b"legacy_channel".to_vec())),
            ZspFrame::BinaryString(Some(b"legacy_payload".to_vec())),
        ];

        let frame = ZspFrame::Array(components);
        let encoded = ZspEncoder::encode(&frame).unwrap();
        let mut data = Bytes::from(encoded);

        let decoded = decode_command(&mut data).unwrap().unwrap();
        match decoded {
            PubSubCommand::Publish(ch, payload) => {
                assert_eq!("legacy_channel", ch);
                match payload {
                    MessagePayload::Bytes(bytes) => {
                        assert_eq!(b"legacy_payload", bytes.as_ref());
                    }
                    _ => panic!("Expected bytes payload for legacy format"),
                }
            }
            _ => panic!("Expected Publish command"),
        }
    }

    #[test]
    fn test_json_publish_command() {
        let json_data = json!({"event": "user_login", "user_id": 42});
        let payload = MessagePayload::Json(json_data.clone());

        let encoded = encode_publish("events", &payload).unwrap();
        let mut data = encoded;
        let decoded_cmd = decode_command(&mut data).unwrap().unwrap();

        match decoded_cmd {
            PubSubCommand::Publish(channel, decoded_payload) => {
                assert_eq!("events", channel);
                match decoded_payload {
                    MessagePayload::Json(decoded_json) => {
                        assert_eq!(json_data, decoded_json);
                    }
                    _ => panic!("Expected JSON payload"),
                }
            }
            _ => panic!("Expected Publish command"),
        }
    }
}
