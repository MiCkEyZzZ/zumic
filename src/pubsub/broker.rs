use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;

use super::{Message, Subscriber};

/// Брокер Pub/Sub сообщений.
pub struct Broker {
    subscribers: Arc<RwLock<HashMap<String, Vec<Subscriber>>>>,
}

impl Broker {
    /// Создатёт новый брокер.
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Подписаться на канал.
    pub fn subscribe(&self, channel: String) -> mpsc::Receiver<Message> {
        let (tx, rx) = mpsc::channel(100); // можно будет сделать capacity настраиваемым

        let mut subs = self.subscribers.write().unwrap();
        subs.entry(channel).or_insert_with(Vec::new).push(tx);

        rx
    }

    /// Публиковать сообщения в канал.
    pub fn publish(&self, channel: &str, payload: Vec<u8>) {
        let subs = self.subscribers.read().unwrap();

        if let Some(subscribers) = subs.get(channel) {
            let message = Message::new(channel, payload);

            for subscriber in subscribers {
                let _ = subscriber.try_send(message.clone());
                // игнорируем ошмбки, если подписчик не читает.
            }
        }
    }

    /// Удалить все подписки на канал.
    pub fn unsubscribe_all(&self, channel: &str) {
        let mut subs = self.subscribers.write().unwrap();
        subs.remove(channel);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;

    /// Тест проверяет, что брокер пустой при создании.
    #[tokio::test]
    async fn test_broker_creation() {
        let broker = Broker::new();
        assert!(broker.subscribers.read().unwrap().is_empty());
    }

    /// Тест проверяет, что происходит подписка → публикация → приходят сообщения.
    #[tokio::test]
    async fn test_subscribe_and_publish() {
        let broker = Broker::new();

        let mut subscriber = broker.subscribe("channel1".to_string());

        broker.publish("channel1", b"hello".to_vec());

        // Ждём сообщение
        let received = timeout(Duration::from_secs(1), subscriber.recv())
            .await
            .expect("Timed out")
            .expect("No message received");

        assert_eq!(received.channel, "channel1");
        assert_eq!(received.payload, b"hello".to_vec());
    }

    /// Тест проверяет, что публикует без подписчиков, без ошибок.
    #[tokio::test]
    async fn test_publish_no_subscribers() {
        let broker = Broker::new();

        // Публикуем в канал без подписчиков.
        broker.publish("no_subscribers", b"test".to_vec());

        // Просто проверяем что не паникует.
        assert!(true);
    }

    /// Тест проверяет, что происходит подписка → удаляется подписка → убеждаемся, что сообщения больше не приходят.
    #[tokio::test]
    async fn test_unsubscribe_all() {
        let broker = Broker::new();

        let mut subscriber = broker.subscribe("channel2".to_string());

        broker.unsubscribe_all("channel2");

        broker.publish("channel2", b"message".to_vec());

        // пробуем получить сообщение.
        let result = timeout(Duration::from_millis(100), subscriber.recv()).await;

        // ожидаем таймаут (ничего не пришло).
        assert!(result.is_err() || result.unwrap().is_none());
    }
}
