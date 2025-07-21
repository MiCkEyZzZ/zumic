use std::sync::Arc;

use globset::Glob;
use tokio::sync::broadcast;

use super::Message;
use crate::{RecvError, TryRecvError};

/// Подписка на конкретный канал по имени.
///
/// Предоставляет удобный async интерфейс для получения сообщений
/// без необходимости обращаться к внутреннему `broadcast::Receiver`.
///
/// Отписка происходит автоматически при `Drop`.
pub struct Subscription {
    /// Название канала, на который подписаны.
    pub channel: Arc<str>,
    /// Внутренний приёмник для входящих сообщений.
    pub(crate) inner: broadcast::Receiver<Message>,
}

/// Подписка на каналы по glob-паттерну.
///
/// Использует [`globset::Glob`] для сопоставления имён каналов
/// и получает сообщения из всех каналов, подходящих под шаблон.
///
/// Отписка происходит автоматически при `Drop`.
pub struct PatternSubscription {
    /// Шаблон glob для сопоставления имён каналов.
    pub pattern: Glob,
    /// Внутренний приёмник для входящих сообщений.
    pub inner: broadcast::Receiver<Message>,
}

impl Subscription {
    /// Асинхронно ожидает следующее сообщение из канала.
    ///
    /// # Возвращает
    /// - `Ok(Message)` при успешном получении сообщения
    /// - `Err(RecvError::Closed)` если канал закрыт
    /// - `Err(RecvError::Lagged(n))` если приёмник отстал на `n` сообщений
    pub async fn recv(&mut self) -> Result<Message, RecvError> {
        self.inner.recv().await.map_err(Into::into)
    }

    /// Пытается получить сообщение без блокировки.
    ///
    /// # Возвращает
    /// - `Ok(Message)` если сообщение доступно немедленно
    /// - `Err(TryRecvError::Empty)` если нет доступных сообщений
    /// - `Err(TryRecvError::Closed)` если канал закрыт
    /// - `Err(TryRecvError::Lagged(n))` если приёмник отстал на `n` сообщений
    pub fn try_recv(&mut self) -> Result<Message, TryRecvError> {
        self.inner.try_recv().map_err(Into::into)
    }

    /// Асинхронно ожидает сообщение с таймаутом.
    ///
    /// # Аргументы
    /// - `timeout` - максимальное время ожидания
    ///
    /// # Возвращает
    /// - `Ok(Message)` при успешном получении сообщения
    /// - `Err(PubSubError::Timeout)` если время ожидания истекло
    /// - `Err(PubSubError::ChannelClosed)` если канал закрыт
    /// - `Err(PubSubError::Lagged(n))` если приёмник отстал
    pub async fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Message, RecvError> {
        match tokio::time::timeout(timeout, self.recv()).await {
            Ok(result) => result,
            Err(_) => Err(RecvError::Timeout),
        }
    }

    /// Возвращает изменяемую ссылку на внутренний приёмник.
    ///
    /// **Deprecated**: Используйте `recv()` и `try_recv()` вместо прямого
    /// доступа к receiver. Этот метод оставлен для обратной совместимости.
    #[deprecated(
        since = "0.2.0",
        note = "Используйте recv() и try_recv() методы вместо прямого доступа к receiver"
    )]
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Явно отписаться от канала. Аналогично `drop(self)`.
    ///
    /// После вызова больше не будут приходить сообщения.
    pub fn unsubscribe(self) {
        // При drop Receiver отписывается сам
    }

    /// Возвращает имя канала, на который подписались.
    pub fn channel_name(&self) -> &Arc<str> {
        &self.channel
    }

    /// Проверяет, закрыт ли канал (нет активных отправителей).
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Возвращает количество сообщений в очереди на получение.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Проверяет, пуста ли очередь сообщений.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl PatternSubscription {
    /// Асинхронно ожидает следующее сообщение, соответствующее паттерну.
    ///
    /// # Возвращает
    /// - `Ok(Message)` при успешном получении сообщения
    /// - `Err(RecvError::Closed)` если канал закрыт
    /// - `Err(RecvError::Lagged(n))` если приёмник отстал на `n` сообщений
    pub async fn recv(&mut self) -> Result<Message, RecvError> {
        self.inner.recv().await.map_err(Into::into)
    }

    /// Пытается получить сообщение без блокировки.
    ///
    /// # Возвращает
    /// - `Ok(Message)` если сообщение доступно немедленно
    /// - `Err(TryRecvError::Empty)` если нет доступных сообщений
    /// - `Err(TryRecvError::ChannelClosed)` если канал закрыт
    /// - `Err(TryRecvError::Lagged(n))` если приёмник отстал на `n` сообщений
    pub fn try_recv(&mut self) -> Result<Message, TryRecvError> {
        self.inner.try_recv().map_err(Into::into)
    }

    /// Асинхронно ожидает сообщение с таймаутом.
    ///
    /// # Аргументы
    /// - `timeout` - максимальное время ожидания
    ///
    /// # Возвращает
    /// - `Ok(Message)` при успешном получении сообщения
    /// - `Err(PubSubError::Timeout)` если время ожидания истекло
    /// - `Err(PubSubError::ChannelClosed)` если канал закрыт
    /// - `Err(PubSubError::Lagged(n))` если приёмник отстал
    pub async fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Message, RecvError> {
        match tokio::time::timeout(timeout, self.recv()).await {
            Ok(result) => result,
            Err(_) => Err(RecvError::Timeout),
        }
    }

    /// Возвращает изменяемую ссылку на внутренний приёмник.
    ///
    /// **Deprecated**: Используйте `recv()` вместо прямого доступа к receiver.
    /// Этот метод оставлен для обратной совместимости.
    #[deprecated(
        since = "0.2.0",
        note = "Используйте recv() метод вместо прямого доступа к receiver"
    )]
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

    /// Возвращает питтерн подписки.
    pub fn pattern(&self) -> &Glob {
        &self.pattern
    }

    /// Возвращает строковое представление паттерна.
    pub fn pattern_str(&self) -> &str {
        self.pattern.glob()
    }

    /// Проверяет, закрыт ли канал (нет активных отправителей).
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Возвращает количество сообщений в очереди на получение.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Проверяет, пуста ли очередь сообщений.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use globset::Glob;
    use tokio::{sync::broadcast, time::timeout};

    use crate::{pubsub::PatternSubscription, Broker, RecvError, Subscription};

    /// Тест проверяет, что поле `channel` содержит правильное
    /// имя канала.
    #[tokio::test]
    async fn test_subscription_channel_name() {
        let sub = {
            let broker = Broker::new(10);
            let sub = broker
                .subscribe("mychan")
                .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
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
    #[allow(deprecated)]
    async fn test_receive_message_via_subscription() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("testchan")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        broker
            .publish("testchan", Bytes::from_static(b"hello"))
            .unwrap();
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
    #[allow(deprecated)]
    async fn test_pattern_subscription_receives_message() {
        let broker = Broker::new(10);
        let mut psub = broker
            .psubscribe("foo*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        broker
            .publish("foobar", Bytes::from_static(b"xyz"))
            .unwrap();

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
    #[allow(deprecated)]
    async fn test_pattern_unsubscribe_stops_reception() {
        let broker = Broker::new(10);
        let mut psub = broker.psubscribe("bar*").unwrap();
        broker.punsubscribe("bar*").unwrap();
        broker.publish_unchecked("barbaz", Bytes::from_static(b"nope"));
        let result = timeout(Duration::from_millis(100), psub.receiver().recv()).await;
        assert!(
            result.is_err() || matches!(result.unwrap(), Err(broadcast::error::RecvError::Closed))
        );
    }

    /// Тест проверяет, что несколько шаблонных подписок получают
    /// одно и то же сообщение.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_multiple_pattern_subscriptions_receive() {
        let broker = Broker::new(10);
        let mut ps1 = broker
            .psubscribe("qu?x")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let mut ps2 = broker
            .psubscribe("qu*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        broker
            .publish("quux", Bytes::from_static(b"hello"))
            .unwrap();

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
    #[allow(deprecated)]
    async fn test_double_subscribe_same_channel() {
        let broker = Broker::new(5);
        let mut a = broker
            .subscribe("dup")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let mut b = broker
            .subscribe("dup")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        broker.publish("dup", Bytes::from_static(b"X")).unwrap();
        assert_eq!(
            a.receiver().recv().await.unwrap().payload,
            Bytes::from_static(b"X")
        );
        assert_eq!(
            b.receiver().recv().await.unwrap().payload,
            Bytes::from_static(b"X")
        );
    }

    #[tokio::test]
    async fn test_subscription_recv_timeout() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("test-channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Тест таймаута
        let result = sub.recv_timeout(Duration::from_millis(50)).await;
        assert!(matches!(result, Err(RecvError::Timeout)));
    }

    #[tokio::test]
    async fn test_pattern_subscription_timeout() {
        let broker = Broker::new(10);
        let mut sub = broker
            .psubscribe("test.*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Тест таймаута
        let result = sub.recv_timeout(Duration::from_millis(50)).await;
        assert!(matches!(result, Err(RecvError::Timeout)));
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

    #[test]
    fn test_subscription_properties() {
        let broker = Broker::new(10);
        let sub = broker
            .subscribe("test-channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        assert_eq!(sub.channel_name().as_ref(), "test-channel");
        assert!(sub.is_empty());
        assert_eq!(sub.len(), 0);
    }

    #[test]
    fn test_pattern_subscription_properties() {
        let broker = Broker::new(10);
        let sub = broker
            .psubscribe("test.*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        assert_eq!(sub.pattern_str(), "test.*");
        assert!(sub.is_empty());
        assert_eq!(sub.len(), 0);
    }
}
