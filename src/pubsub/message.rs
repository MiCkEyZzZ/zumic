#[derive(Debug, Clone)]
pub struct Message {
    pub channel: String,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new(channel: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            channel: channel.into(),
            payload: payload.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет создание сообщения с &str и &[u8]
    #[test]
    fn test_message_creation_with_str_and_bytes() {
        let ch = "news";
        let pl = b"hello world".to_vec();

        let msg = Message::new(ch, pl.clone());

        assert_eq!(msg.channel, "news");
        assert_eq!(msg.payload, pl);
    }

    /// Тест проверяет создание сообщения с String и Vec<u8>
    #[test]
    fn test_message_creation_with_string_and_vec() {
        let ch = String::from("updates");
        let pl = vec![1, 2, 3, 4, 5];

        let msg = Message::new(ch.clone(), pl.clone());

        assert_eq!(msg.channel, "updates");
        assert_eq!(msg.payload, pl);
    }

    /// Тест проверяет создание сообщения с пустым каналом
    #[test]
    fn test_message_with_empty_channel() {
        let ch = "";
        let pl = b"data".to_vec();

        let msg = Message::new(ch, pl.clone());

        assert_eq!(msg.channel, "");
        assert_eq!(msg.payload, pl);
    }

    /// Тест проверяет создание сообщения с пустым содержимым
    #[test]
    fn test_message_with_empty_payload() {
        let ch = "system";
        let pl: Vec<u8> = vec![];

        let msg = Message::new(ch, pl.clone());

        assert_eq!(msg.channel, "system");
        assert_eq!(msg.payload, pl);
    }

    /// Тест проверяет создание сообщения с бинарными данными в payload
    #[test]
    fn test_message_with_binary_payload() {
        let ch = "bin";
        let pl = vec![0, 255, 128, 64, 0];

        let msg = Message::new(ch, pl.clone());

        assert_eq!(msg.channel, "bin");
        assert_eq!(msg.payload, pl);
    }
}
