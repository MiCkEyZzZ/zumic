use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::intern_channel;
use crate::RecvError;

/// Различные типы полезной нагрузки сообщений.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagePayload {
    /// Сырые байты - самый эффективный формат.
    Bytes(Bytes),
    /// UTF-8 строка.
    String(String),
    /// JSON значение (сериализуется в компактном формате).
    Json(JsonValue),
    /// Пользовательский тип, сериализованный через serde.
    Serialized { data: Bytes, content_type: String },
}

/// Форматы сериализации, поддерживаемые системой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    /// JSON - человекочитаемый, но менее эффективный
    Json,
    /// Bincode - быстрый бинарный формат
    Bincode,
    /// MessagePack - компактный бинарный формат
    MessagePack,
}

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
    pub payload: MessagePayload,
    /// Метаданные сообщения (опционально)
    pub metadata: Option<MessageMetadata>,
}

/// Метаданные сообщения для дополнительной информации.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    /// ID сообщения (если нужно)
    pub message_id: Option<String>,
    /// Временная метка отправки (Unix timestamp в миллисекундах)
    pub timestamp: Option<u64>,
    /// Пользовательские заголовки
    pub headers: HashMap<String, String>,
}

impl Message {
    /// Создаёт новое сообщение из канала и полезной нагрузки.
    ///
    /// Параметр `channel` может быть любого типа, реализующего
    /// `AsRef<str>` (`&str`, `String`, `Arc<str>`),
    /// а `payload` — любого типа, преобразуемого в `Bytes`
    /// (`Vec<u8>`, `&[u8]`, `Bytes`).
    pub fn new<S, P>(
        channel: S,
        payload: P,
    ) -> Self
    where
        S: AsRef<str>,
        P: Into<Bytes>,
    {
        Self {
            channel: intern_channel(channel),
            payload: MessagePayload::Bytes(payload.into()),
            metadata: None,
        }
    }

    /// Создаёт новое сообщение с заданным типом payload.
    pub fn with_payload<S>(
        channel: S,
        payload: MessagePayload,
    ) -> Self
    where
        S: AsRef<str>,
    {
        Self {
            channel: intern_channel(channel),
            payload,
            metadata: None,
        }
    }

    /// Создаёт сообщение из строки.
    pub fn from_string<S, T>(
        channel: S,
        content: T,
    ) -> Self
    where
        S: AsRef<str>,
        T: Into<String>,
    {
        Self {
            channel: intern_channel(channel),
            payload: MessagePayload::String(content.into()),
            metadata: None,
        }
    }

    /// Создаёт сообщение из JSON значения.
    pub fn from_json<S>(
        channel: S,
        json: JsonValue,
    ) -> Self
    where
        S: AsRef<str>,
    {
        Self {
            channel: intern_channel(channel),
            payload: MessagePayload::Json(json),
            metadata: None,
        }
    }

    /// Создаёт сообщение из сериализуемого объекта.
    pub fn from_serializable<S, T>(
        channel: S,
        value: &T,
        format: SerializationFormat,
    ) -> Result<Self, RecvError>
    where
        S: AsRef<str>,
        T: Serialize,
    {
        let payload = MessagePayload::from_serializable(value, format)?;
        Ok(Self {
            channel: intern_channel(channel),
            payload,
            metadata: None,
        })
    }

    /// Создаёт сообщение из полностью статических данных без
    /// копирования.
    ///
    /// Это самый быстрый способ создать сообщение, если и канал,
    /// и содержимое имеют
    /// статическую область видимости (`'static`).
    pub fn from_static(
        channel: &'static str,
        payload: &'static [u8],
    ) -> Self {
        Self {
            channel: intern_channel(channel),
            payload: MessagePayload::Bytes(Bytes::from_static(payload)),
            metadata: None,
        }
    }

    /// Добавляет метаданные к сообщению.
    pub fn with_metadata(
        mut self,
        metadata: MessageMetadata,
    ) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Добавляет временную метку к сообщению.
    pub fn with_timestamp(mut self) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut metadata = self.metadata.unwrap_or_default();
        metadata.timestamp = Some(timestamp);
        self.metadata = Some(metadata);
        self
    }

    /// Добавляет ID сообщения
    pub fn with_message_id<S: Into<String>>(
        mut self,
        id: S,
    ) -> Self {
        let mut metadata = self.metadata.unwrap_or_default();
        metadata.message_id = Some(id.into());
        self.metadata = Some(metadata);
        self
    }

    /// Добавляет пользовательский заголовок.
    pub fn with_header<K, V>(
        mut self,
        key: K,
        value: V,
    ) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let mut metadata = self.metadata.unwrap_or_default();
        metadata.headers.insert(key.into(), value.into());
        self.metadata = Some(metadata);
        self
    }

    /// Преобразует сообщение в байты для передачи по сети
    pub fn to_bytes(&self) -> Result<Bytes, RecvError> {
        self.payload.to_bytes()
    }

    /// Десериализует payload сообщения в заданный тип
    pub fn deserialize<T>(&self) -> Result<T, RecvError>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.payload.deserialize()
    }

    /// Возвращает размер сообщения в байтах
    pub fn size(&self) -> usize {
        self.payload.len()
            + self.channel.len()
            + self
                .metadata
                .as_ref()
                .map(|m| serde_json::to_string(m).map(|s| s.len()).unwrap_or(0))
                .unwrap_or(0)
    }
}

impl MessagePayload {
    /// Создаёт payload из байтов.
    pub fn from_bytes<B: Into<Bytes>>(bytes: B) -> Self {
        Self::Bytes(bytes.into())
    }

    /// Создаёт payload из строки.
    pub fn from_string<S: Into<String>>(s: S) -> Self {
        Self::String(s.into())
    }

    /// Создаёт payload из строки.
    pub fn from_json(value: JsonValue) -> Self {
        Self::Json(value)
    }

    /// Создаёт payload из сериализуемого объекта.
    pub fn from_serializable<T>(
        value: &T,
        format: SerializationFormat,
    ) -> Result<Self, RecvError>
    where
        T: Serialize,
    {
        let (data, content_type) = match format {
            SerializationFormat::Json => {
                let json_bytes = serde_json::to_vec(value)
                    .map_err(|e| RecvError::SerializationError(e.to_string()))?;
                (Bytes::from(json_bytes), "application/json".to_string())
            }
            SerializationFormat::Bincode => {
                let bincode_bytes = bincode::serialize(value)
                    .map_err(|e| RecvError::SerializationError(e.to_string()))?;
                (
                    Bytes::from(bincode_bytes),
                    "application/bincode".to_string(),
                )
            }
            SerializationFormat::MessagePack => {
                let msgpack_bytes = rmp_serde::to_vec(value)
                    .map_err(|e| RecvError::SerializationError(e.to_string()))?;
                (
                    Bytes::from(msgpack_bytes),
                    "application/msgpack".to_string(),
                )
            }
        };
        Ok(Self::Serialized { data, content_type })
    }

    /// Конвертируем payload в байт для передачи.
    pub fn to_bytes(&self) -> Result<Bytes, RecvError> {
        match self {
            Self::Bytes(bytes) => Ok(bytes.clone()),
            Self::String(s) => Ok(Bytes::from(s.as_bytes().to_vec())),
            Self::Json(json) => {
                let json_str = serde_json::to_string(json)
                    .map_err(|e| RecvError::SerializationError(e.to_string()))?;
                Ok(Bytes::from(json_str.into_bytes()))
            }
            Self::Serialized { data, .. } => Ok(data.clone()),
        }
    }

    /// Десериализует payload в заданный тип
    pub fn deserialize<T>(&self) -> Result<T, RecvError>
    where
        T: for<'de> Deserialize<'de>,
    {
        match self {
            Self::Json(json) => serde_json::from_value(json.clone())
                .map_err(|e| RecvError::SerializationError(e.to_string())),
            Self::String(s) => {
                serde_json::from_str(s).map_err(|e| RecvError::SerializationError(e.to_string()))
            }
            Self::Bytes(bytes) => serde_json::from_slice(bytes)
                .map_err(|e| RecvError::SerializationError(e.to_string())),
            Self::Serialized { data, content_type } => match content_type.as_str() {
                "application/json" => serde_json::from_slice(data)
                    .map_err(|e| RecvError::SerializationError(e.to_string())),
                "application/bincode" => bincode::deserialize(data)
                    .map_err(|e| RecvError::SerializationError(e.to_string())),
                "application/msgpack" => rmp_serde::from_slice(data)
                    .map_err(|e| RecvError::SerializationError(e.to_string())),
                _ => Err(RecvError::SerializationError(format!(
                    "Unsupported content type: {content_type}"
                ))),
            },
        }
    }

    /// Возвращает размер payload в байтах.
    pub fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::String(s) => s.len(),
            Self::Json(json) => {
                // приблизительно оценка размера JSON
                serde_json::to_string(json).map(|s| s.len()).unwrap_or(0)
            }
            Self::Serialized { data, .. } => data.len(),
        }
    }

    /// Проверяет, пуст ли payload.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Возвращает тип содержимого как строку.
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Bytes(_) => "application/octet-stream",
            Self::String(_) => "text/plain",
            Self::Json(_) => "application/json",
            Self::Serialized { content_type, .. } => {
                // возвращаем статическую строку для основных типов.
                match content_type.as_str() {
                    "application/json" => "application/json",
                    "application/bincode" => "application/bincode",
                    "application/msgpack" => "application/msgpack",
                    _ => "application/octet-stream",
                }
            }
        }
    }
}

impl From<&str> for MessagePayload {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for MessagePayload {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<Vec<u8>> for MessagePayload {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(Bytes::from(bytes))
    }
}

impl From<Bytes> for MessagePayload {
    fn from(bytes: Bytes) -> Self {
        Self::Bytes(bytes)
    }
}

impl From<JsonValue> for MessagePayload {
    fn from(json: JsonValue) -> Self {
        Self::Json(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestStruct {
        id: u32,
        name: String,
        active: bool,
    }

    /// Тест проверяет создание сообщения из строки и вектора:
    /// правильность канала и преобразование полезной нагрузки
    /// в `Bytes`.
    #[test]
    fn test_from_and_vec() {
        let ch = "news";
        let pl_vec = vec![1, 2, 3];
        let msg: Message = Message::new(ch, pl_vec.clone());
        assert_eq!(&*msg.channel, "news");
        assert_eq!(msg.payload, MessagePayload::Bytes(Bytes::from(pl_vec)));
    }

    /// Тест проверяет создание из `String` и `Bytes`, включая
    /// совпадение ссылок и содержимого.
    #[test]
    fn new_from_string_and_bytes() {
        let ch_string = String::from("updates");
        let pl_bytes = Bytes::from_static(b"hello");
        let msg: Message = Message::new(ch_string.clone(), pl_bytes.clone());
        assert_eq!(&*msg.channel, &ch_string);
        assert_eq!(msg.payload, MessagePayload::Bytes(pl_bytes));
    }

    /// Тест проверяет, что клонирование сохраняет указатели (Arc
    /// и Bytes) без копирования.
    #[test]
    fn clone_preserves_arc_and_bytes_zero_copy() {
        let msg1 = Message::new("chan", Bytes::from_static(b"x"));
        let arc_ptr = Arc::as_ptr(&msg1.channel);
        let bytes_ptr = if let MessagePayload::Bytes(ref b) = msg1.payload {
            b.as_ptr()
        } else {
            panic!("payload is not Bytes");
        };

        let msg2 = msg1.clone();
        let bytes_ptr2 = if let MessagePayload::Bytes(ref b) = msg2.payload {
            b.as_ptr()
        } else {
            panic!("payload is not Bytes");
        };

        assert_eq!(Arc::as_ptr(&msg2.channel), arc_ptr);
        assert_eq!(bytes_ptr2, bytes_ptr);
    }

    /// Тест проверяет создание из статических данных без
    /// копирования (`from_static`).
    #[test]
    fn from_static_zero_copy() {
        let msg = Message::from_static("static_chan", b"data");
        assert_eq!(&*msg.channel, "static_chan");
        assert_eq!(
            msg.payload,
            MessagePayload::Bytes(Bytes::from_static(b"data"))
        );
    }

    /// Тест проверяет сравние поведения `new` и `from_static`:
    /// каналы равны по значению, а указатели совпадают из-за
    /// интернирования.
    #[test]
    fn mix_new_and_from_static() {
        let m1 = Message::new("kin", b"dzadza".to_vec());
        let m2 = Message::from_static("kin", b"dzadza");
        assert_eq!(&*m1.channel, &*m2.channel);
        assert_eq!(m1.payload, m2.payload);
        assert!(Arc::ptr_eq(&m1.channel, &m2.channel));
    }

    /// Тест проверяет корректную работу с пустым каналом и
    /// полезной нагрузкой (new и from_static).
    #[test]
    fn empty_channel_and_payload() {
        let m = Message::new("", Vec::<u8>::new());
        assert_eq!(&*m.channel, "");
        assert!(m.payload.is_empty());

        let m_static = Message::from_static("", b"");
        assert_eq!(&*m_static.channel, "");
        assert!(m_static.payload.is_empty());
    }

    /// Тест проверяет создание из среза и `Bytes`, сравнивая
    /// полезную нагрузку.
    #[test]
    fn new_from_slice_and_bytes_clone() {
        // slice имеет тип &[u8; 10]
        let slice: &'static [u8; 10] = b"slice_data";

        let m1 = Message::new("chan1", &slice[..]);
        let expected1 = MessagePayload::Bytes(Bytes::from_static(&slice[..]));

        let bytes = Bytes::from_static(b"bytes_data");
        let m2 = Message::new("chan2", bytes.clone());
        let expected2 = MessagePayload::Bytes(bytes.clone());

        assert_eq!(m1.payload, expected1);
        assert_eq!(m2.payload, expected2);
    }

    /// Тест проверяет создание из вектора и статического среза,
    /// сравнивая полезную нагрузку с ожидаемой.
    #[test]
    fn new_from_vec_and_static() {
        let v = vec![9u8; 10];
        let s: &[u8] = &[1, 2, 3];
        let m1 = Message::new("v", v.clone());
        let m2 = Message::new("s", s);
        assert_eq!(m1.payload, MessagePayload::Bytes(Bytes::from(v)));
        assert_eq!(m2.payload, MessagePayload::Bytes(Bytes::from_static(s)));
    }

    /// Тест проверяет, что два сообщения с одинаковыми каналами
    /// и полезной нагрузкой равны.
    #[test]
    fn message_equality() {
        let a = Message::new("a", b"x".to_vec());
        let b = Message::new("a", b"x".to_vec());
        assert_eq!(a, b);
    }

    /// Тест проверяет, что `Debug` вывод содержит канал и полезную
    ///  нагрузку.
    #[test]
    fn debug_contains_channel_and_payload() {
        let m = Message::new("dbg", b"z".to_vec());
        let s = format!("{m:?}");
        assert!(s.contains("channel"));
        assert!(s.contains("payload"));
        assert!(s.contains("dbg"));
    }

    /// Тест проверяет, что клонирование большого payload не
    /// копирует данные (zero-copy).
    #[test]
    fn large_payload_clone_zero_copy() {
        let big = vec![0u8; 1_000_000];
        let m1 = Message::new("big", big.clone());

        // Распаковываем Bytes и берём указатель
        let ptr1 = if let MessagePayload::Bytes(ref b) = m1.payload {
            b.as_ptr()
        } else {
            panic!("Expected Bytes payload");
        };

        // Клонируем сообщение
        let m2 = m1.clone();

        // И снова распаковываем и берём указатель
        let ptr2 = if let MessagePayload::Bytes(ref b) = m2.payload {
            b.as_ptr()
        } else {
            panic!("Expected Bytes payload");
        };

        // Проверяем, что указатели совпадают и длина корректна
        assert_eq!(ptr2, ptr1);
        assert_eq!(m2.payload.len(), big.len());
    }

    /// Тест проверяет, что создание из `Arc<str>` сохраняет
    /// указатель.
    #[test]
    fn new_from_arc_str_retains_pointer() {
        let arc: Arc<str> = Arc::from("mychan");
        let m = Message::new(arc.clone(), b"p".to_vec());
        assert_eq!(&*arc, &*m.channel);
    }

    /// Тест проверяет, что вызовы `from_static` с одинаковыми
    /// именами каналов возвращают один и тот же `Arc<str>`,
    /// несмотря на разные полезные нагрузки.
    #[test]
    fn static_messages_share_pointer() {
        let m1 = Message::from_static("stat", b"1");
        let m2 = Message::from_static("stat", b"2");
        assert!(
            Arc::ptr_eq(&m1.channel, &m2.channel),
            "Identical static channels should intern to the same Arc"
        );
    }

    /// Тест проверяет, что `Message::new` и `Message::from_static`
    /// с одинаковыми именами используют один и тот же
    /// интернированный `Arc<str>` для канала.
    #[test]
    fn new_and_from_static_share_pointer() {
        let m1 = Message::new("mix", b"kin".to_vec());
        let m2 = Message::from_static("mix", b"dza");
        assert!(
            Arc::ptr_eq(&m1.channel, &m2.channel),
            "new and from_static with the same name should return the same Arc"
        );
    }

    #[test]
    fn test_bytes_payload() {
        let data = b"Hello, World!";
        let msg = Message::new("test_channel", data.as_slice());

        assert_eq!(msg.channel.as_ref(), "test_channel");
        assert_eq!(msg.payload.len(), 13);
        assert_eq!(msg.payload.content_type(), "application/octet-stream");
    }

    #[test]
    fn test_string_payload() {
        let msg = Message::from_string("test_channel", "Hello, Rust!");

        assert_eq!(msg.channel.as_ref(), "test_channel");
        assert_eq!(msg.payload.len(), 12);
        assert_eq!(msg.payload.content_type(), "text/plain");
    }

    #[test]
    fn test_json_payload() {
        let json_data = serde_json::json!({
            "message": "Hello",
            "number": 42
        });

        let msg = Message::from_json("test_channel", json_data.clone());

        assert_eq!(msg.channel.as_ref(), "test_channel");
        assert_eq!(msg.payload.content_type(), "application/json");

        if let MessagePayload::Json(json) = &msg.payload {
            assert_eq!(json["message"], "Hello");
            assert_eq!(json["number"], 42);
        } else {
            panic!("Expected JSON payload");
        }
    }

    #[test]
    fn test_serializable_payload() {
        let test_data = TestStruct {
            id: 123,
            name: "Test".to_string(),
            active: true,
        };

        let msg = Message::from_serializable("test_channel", &test_data, SerializationFormat::Json)
            .unwrap();

        let deserialized: TestStruct = msg.deserialize().unwrap();
        assert_eq!(deserialized, test_data);
    }

    #[test]
    fn test_message_with_metadata() {
        let msg = Message::from_string("test_channel", "Hello")
            .with_timestamp()
            .with_message_id("msg_001")
            .with_header("source", "test_system")
            .with_header("priority", "high");

        let metadata = msg.metadata.as_ref().unwrap();
        assert_eq!(metadata.message_id, Some("msg_001".to_string()));
        assert!(metadata.timestamp.is_some());
        assert_eq!(
            metadata.headers.get("source"),
            Some(&"test_system".to_string())
        );
        assert_eq!(metadata.headers.get("priority"), Some(&"high".to_string()));
    }

    #[test]
    fn test_payload_conversion() {
        let original = "Hello, World!";
        let payload = MessagePayload::from_string(original);
        let bytes = payload.to_bytes().unwrap();

        assert_eq!(bytes, Bytes::from(original.as_bytes()));
    }

    #[test]
    fn test_bincode_serialization() {
        let test_data = TestStruct {
            id: 456,
            name: "Bincode Test".to_string(),
            active: false,
        };

        let msg =
            Message::from_serializable("bincode_channel", &test_data, SerializationFormat::Bincode)
                .unwrap();

        let deserialized: TestStruct = msg.deserialize().unwrap();
        assert_eq!(deserialized, test_data);
    }
}
