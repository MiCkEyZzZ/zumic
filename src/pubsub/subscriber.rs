use std::sync::Arc;

use tokio::sync::broadcast;

use super::Message;

/// Обёртка над broadcast::Receiver.
/// При Drop (или вызове unsubscribe) просто дропает внутренний Receiver,
/// и клиент автоматически отписывается.
pub struct Subscription {
    /// Arc<str> того же канала, что и в брокере
    pub channel: Arc<str>,
    pub inner: broadcast::Receiver<Message>,
}

impl Subscription {
    /// Mutable-ссылка на Receiver, чтобы вызывать .recv().
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Явная отписка — просто дропаем Subscription.
    pub fn unsubscribe(self) {
        // ничего не нужно делать: при дропе inner Receiver убирается из broadcast
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use tokio::{sync::broadcast, time::timeout};

    use crate::Subscription;

    use super::super::Broker;

    #[tokio::test]
    async fn test_subscription_channel_name() {
        let sub = {
            let broker = Broker::new(10);
            let sub = broker.subscribe("mychan");
            assert_eq!(&*sub.channel, "mychan");
            sub
        };
        // После выхода брокера из области видимости подписчик всё ещё хранит канал
        assert_eq!(&*sub.channel, "mychan");
    }

    #[tokio::test]
    async fn test_receive_message_via_subscription() {
        let broker = Broker::new(10);
        let mut sub = broker.subscribe("testchan");
        // публикуем payload
        broker.publish("testchan", Bytes::from_static(b"hello"));
        // ожидаем не дольше 100 мс.
        let msg = timeout(Duration::from_millis(100), sub.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg.channel, "testchan");
        assert_eq!(msg.payload, Bytes::from_static(b"hello"));
    }

    #[test]
    fn test_unsubscribe_drops_receiver() {
        // Создадим broadcast-канал вручную
        let (tx, rx) = broadcast::channel(5);
        let channel_arc: Arc<str> = Arc::from("foo");
        // Создаем Subscription прямо
        let sub = Subscription {
            channel: channel_arc.clone(),
            inner: rx,
        };
        assert_eq!(tx.receiver_count(), 1);
        // Drop subscription → receiver_count должно стать 0
        drop(sub);
        assert_eq!(tx.receiver_count(), 0);
    }

    #[test]
    fn test_explicit_unsubscribe_consumes_subscription() {
        let (tx, rx) = broadcast::channel(5);
        let channel_arc: Arc<str> = Arc::from("bar");
        let sub = Subscription {
            channel: channel_arc,
            inner: rx,
        };
        assert_eq!(tx.receiver_count(), 1);
        // вызываем метод, который просто дропает self
        sub.unsubscribe();
        assert_eq!(tx.receiver_count(), 0);
    }
}
