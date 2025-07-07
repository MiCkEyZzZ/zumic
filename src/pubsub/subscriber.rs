use std::sync::Arc;

use globset::Glob;
use tokio::sync::broadcast;

use super::Message;

/// Подписка на конкретный канал Pub/Sub.
///
/// Обёртка над [`broadcast::Receiver`], привязанная к имени
/// канала (`Arc<str>`), позволяет получать сообщения из этого
/// канала.
///
/// Отписка происходит автоматически при `Drop` или явно через
/// [`Subscription::unsubscribe`].
pub struct Subscription {
    /// Название канала, на который подписаны.
    pub channel: Arc<str>,
    /// Внутренний приёмник для входящих сообщений.
    pub inner: broadcast::Receiver<Message>,
}

/// Подписка по шаблону на несколько каналов.
///
/// Использует [`globset::Glob`] для сопоставления имён каналов
/// и получает сообщения из всех каналов, подходящих под шаблон.
///
/// Отписка также происходит автоматически при `Drop` или явно
/// через [`PatternSubscription::unsubscribe`].
pub struct PatternSubscription {
    /// Шаблон glob для сопоставления имён каналов.
    pub pattern: Glob,
    /// Внутренний приёмник для входящих сообщений.
    pub inner: broadcast::Receiver<Message>,
}

impl Subscription {
    /// Возвращает изменяемую ссылку на внутренний приёмник,
    /// чтобы можно было вызвать `.recv().await`.
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Явно отписаться от канала. Аналогично `drop(self)`.
    ///
    /// После вызова больше не будут приходить сообщения.
    pub fn unsubscribe(self) {
        // Ничего не нужно делать: при drop Receiver отписывается сам
    }
}

impl PatternSubscription {
    /// Возвращает изменяемую ссылку на внутренний приёмник,
    /// чтобы можно было вызвать `.recv().await`.
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Явно отписаться от шаблона.
    ///
    /// После вызова больше не будут приходить сообщения по
    /// этому шаблону.
    pub fn unsubscribe(self) {
        // При drop Receiver отписывается сам
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use globset::Glob;
    use tokio::{sync::broadcast, time::timeout};

    use crate::{pubsub::PatternSubscription, Broker, Subscription};

    /// Тест проверяет, что поле `channel` содержит правильное
    /// имя канала.
    #[tokio::test]
    async fn test_subscription_channel_name() {
        let sub = {
            let broker = Broker::new(10);
            let sub = broker.subscribe("mychan");
            assert_eq!(&*sub.channel, "mychan");
            sub
        };
        // Даже после того, как broker вышел из области видимости,
        // имя канала остаётся доступным
        assert_eq!(&*sub.channel, "mychan");
    }

    /// Тест проверяет, что опубликованное сообщение приходит
    /// подписчику.
    #[tokio::test]
    async fn test_receive_message_via_subscription() {
        let broker = Broker::new(10);
        let mut sub = broker.subscribe("testchan");
        broker.publish("testchan", Bytes::from_static(b"hello"));
        let msg = timeout(Duration::from_millis(100), sub.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg.channel, "testchan");
        assert_eq!(msg.payload, Bytes::from_static(b"hello"));
    }

    /// Тест проверяет, что дроп подписки уменьшает счётчик слушателей.
    #[test]
    fn test_unsubscribe_drops_receiver() {
        let (tx, rx) = broadcast::channel(5);
        let channel_arc: Arc<str> = Arc::from("foo");
        let sub = Subscription {
            channel: channel_arc.clone(),
            inner: rx,
        };
        assert_eq!(tx.receiver_count(), 1);
        drop(sub);
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Тест проверяет, что метод `unsubscribe` явно отписывает
    /// подписчика.
    #[test]
    fn test_explicit_unsubscribe_consumes_subscription() {
        let (tx, rx) = broadcast::channel(5);
        let channel_arc: Arc<str> = Arc::from("bar");
        let sub = Subscription {
            channel: channel_arc,
            inner: rx,
        };
        assert_eq!(tx.receiver_count(), 1);
        sub.unsubscribe();
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Тест проверяет, что шаблонная подписка получает сообщение.
    #[tokio::test]
    async fn test_pattern_subscription_receives_message() {
        let broker = Broker::new(10);
        let mut psub = broker.psubscribe("foo*").unwrap();

        broker.publish("foobar", Bytes::from_static(b"xyz"));

        let msg = timeout(Duration::from_millis(100), psub.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");

        assert_eq!(&*msg.channel, "foobar");
        assert_eq!(msg.payload, Bytes::from_static(b"xyz"));
    }

    /// Тест проверяет, что после `punsubscribe` сообщения не
    /// приходят.
    #[tokio::test]
    async fn test_pattern_unsubscribe_stops_reception() {
        let broker = Broker::new(10);
        let mut psub = broker.psubscribe("bar*").unwrap();
        broker.punsubscribe("bar*").unwrap();
        broker.publish("barbaz", Bytes::from_static(b"nope"));
        let result = timeout(Duration::from_millis(100), psub.receiver().recv()).await;
        assert!(
            result.is_err() || matches!(result.unwrap(), Err(broadcast::error::RecvError::Closed))
        );
    }

    /// Тест проверяет, что несколько шаблонных подписок получают
    /// одно и то же сообщение.
    #[tokio::test]
    async fn test_multiple_pattern_subscriptions_receive() {
        let broker = Broker::new(10);
        let mut ps1 = broker.psubscribe("qu?x").unwrap();
        let mut ps2 = broker.psubscribe("qu*").unwrap();

        broker.publish("quux", Bytes::from_static(b"hello"));

        let msg1 = timeout(Duration::from_millis(100), ps1.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg1.channel, "quux");
        assert_eq!(msg1.payload, Bytes::from_static(b"hello"));

        let msg2 = timeout(Duration::from_millis(100), ps2.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg2.channel, "quux");
        assert_eq!(msg2.payload, Bytes::from_static(b"hello"));
    }

    /// Тест проверяет, что дроп шаблонной подписки уменьшает
    /// число слушателей.
    #[test]
    fn test_pattern_unsubscribe_drops_receiver() {
        let (tx, rx) = broadcast::channel(3);
        let pattern = Glob::new("pat*").unwrap();

        let psub = PatternSubscription { pattern, inner: rx };
        assert_eq!(tx.receiver_count(), 1);
        drop(psub);
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Тест проверяет, что метод `unsubscribe` у шаблонной
    /// подписки отписывает.
    #[test]
    fn test_pattern_explicit_unsubscribe_consumes() {
        let (tx, rx) = broadcast::channel(3);
        let pattern = Glob::new("z*").unwrap();

        let psub = PatternSubscription { pattern, inner: rx };
        assert_eq!(tx.receiver_count(), 1);
        psub.unsubscribe();
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Тест проверяет, что две подписки на один канал обе
    /// получают сообщения.
    #[tokio::test]
    async fn test_double_subscribe_same_channel() {
        let broker = Broker::new(5);
        let mut a = broker.subscribe("dup");
        let mut b = broker.subscribe("dup");
        broker.publish("dup", Bytes::from_static(b"X"));
        assert_eq!(
            a.receiver().recv().await.unwrap().payload,
            Bytes::from_static(b"X")
        );
        assert_eq!(
            b.receiver().recv().await.unwrap().payload,
            Bytes::from_static(b"X")
        );
    }

    /// Тест проверяет, что отписка от несуществующего канала
    /// или шаблона не паникует.
    #[test]
    fn test_unsubscribe_nonexistent() {
        let broker = Broker::new(5);
        // оба должны просто вернуться без паники.
        broker.unsubscribe_all("nochan");
        broker.punsubscribe("no*pat").unwrap();
    }

    /// Тест проверяет, что при некорректном шаблоне возвращается
    /// ошибка.
    #[test]
    fn test_invalid_glob_pattern() {
        let broker = Broker::new(5);
        assert!(broker.psubscribe("[invalid[").is_err());
    }
}
