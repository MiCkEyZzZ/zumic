use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use bytes::Bytes;
use dashmap::DashMap;
use tokio::sync::broadcast;

use super::{Message, Subscription};

type ChannelKey = Arc<str>;

/// Брокер Pub/Sub сообщений.
pub struct Broker {
    /// Шардированная карта: канал → Sender
    channels: Arc<DashMap<ChannelKey, broadcast::Sender<Message>>>,
    default_capacity: usize,
    /// Счётчик всех publish-вызовов.
    pub publish_count: AtomicUsize,
    /// Счётчик неудачных send (нет подписчиков).
    pub send_error_count: AtomicUsize,
}

impl Broker {
    /// Создаёт брокер с буфером capacity для каждого канала.
    pub fn new(default_capacity: usize) -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
            default_capacity,
            publish_count: AtomicUsize::new(0),
            send_error_count: AtomicUsize::new(0),
        }
    }

    /// Подписаться на канал. Аллокация Arc<str> делается только здесь.
    pub fn subscribe(&self, channel: &str) -> Subscription {
        let key: Arc<str> = Arc::from(channel);

        // получаем или создаём Sender для key
        let sender = self
            .channels
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(self.default_capacity).0)
            .clone();

        // создаём новый Receiver и кладём в Subscription
        let rx = sender.subscribe();
        Subscription {
            channel: key,
            inner: rx,
        }
    }

    /// Публиковать сообщения в канал.
    pub fn publish(&self, channel: &str, payload: Bytes) {
        self.publish_count.fetch_add(1, Ordering::Relaxed);

        if let Some(entry) = self.channels.get_mut(channel) {
            let sender = entry.value().clone();
            let msg = Message::new(entry.key().clone(), payload);

            // отправляем всем подписчикам.
            if sender.send(msg).is_err() {
                self.send_error_count.fetch_add(1, Ordering::Relaxed);
            }

            // если после этого никого нет - удаляем канал.
            if sender.receiver_count() == 0 {
                let key = entry.key().clone();
                drop(entry);
                self.channels.remove(&*key);
            }
        }
    }

    /// Удалить все подписки на канал (закрыть канал).
    pub fn unsubscribe_all(&self, channel: &str) {
        self.channels.remove(channel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio::time::{timeout, Duration};

    /// Helper: создаёт брокер и сразу подписывается, возвращая (broker, receiver)
    async fn setup_one() -> (Broker, tokio::sync::broadcast::Receiver<Message>) {
        let broker = Broker::new(5);
        let Subscription { inner: rx, .. } = broker.subscribe("chan");
        (broker, rx)
    }

    #[tokio::test]
    async fn test_publish_and_receive() {
        let (broker, mut rx) = setup_one().await;
        broker.publish("chan", Bytes::from_static(b"x"));
        let msg = timeout(Duration::from_millis(50), rx.recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg.channel, "chan");
        assert_eq!(msg.payload, Bytes::from_static(b"x"));
        // publish_count должно быть 1, send_error_count == 0
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_publish_to_nonexistent_channel() {
        let broker = Broker::new(5);
        broker.publish("nochan", Bytes::from_static(b"z"));
        // Нет подписчиков, канал не создаётся, send_error не инкрементится
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("nochan"));
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive() {
        let broker = Broker::new(5);
        let subs = (0..3)
            .map(|_| broker.subscribe("multi"))
            .map(|s| s.inner)
            .collect::<Vec<_>>();

        broker.publish("multi", Bytes::from_static(b"d"));
        for mut rx in subs {
            let msg = timeout(Duration::from_millis(50), rx.recv())
                .await
                .expect("timed out")
                .expect("no msg");
            assert_eq!(&*msg.channel, "multi");
            assert_eq!(msg.payload, Bytes::from_static(b"d"));
        }
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_auto_remove_empty_channel_and_error_count() {
        // 1) подписываемся и сразу дропаем подписку
        let broker = Broker::new(5);
        {
            let sub = broker.subscribe("temp");
            drop(sub);
        }
        // канал всё ещё есть до первой публикации
        assert!(broker.channels.contains_key("temp"));

        // 2) публикация должна дать send_error и удалить канал
        broker.publish("temp", Bytes::from_static(b"u"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 1);
        assert!(!broker.channels.contains_key("temp"));
    }

    #[tokio::test]
    async fn test_unsubscribe_all() {
        let broker = Broker::new(5);
        let _sub = broker.subscribe("gone");
        // теперь удаляем все подписки
        broker.unsubscribe_all("gone");
        assert!(!broker.channels.contains_key("gone"));

        // публикация после полного удаления — publish_count инкрементируется,
        // но send_error_count не, и канал не создаётся заново
        broker.publish("gone", Bytes::from_static(b"x"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("gone"));
    }
}
