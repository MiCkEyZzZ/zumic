use std::sync::Arc;

use globset::Glob;
use tokio::sync::broadcast;

use super::Message;

/// Подписка на конкретный канал pub/sub.
///
/// Оборачивает [`broadcast::Receiver`], ассоциированный с именем канала (`Arc<str>`),
/// позволяя получать сообщения из этого канала.
///
/// Отписка происходит автоматически при `Drop`, либо явно через [`Subscription::unsubscribe`].
pub struct Subscription {
    /// Имя канала, на который выполнена подписка.
    pub channel: Arc<str>,
    /// Внутренний `broadcast::Receiver`, через который приходят сообщения.
    pub inner: broadcast::Receiver<Message>,
}

/// Подписка на каналы по шаблону (pattern-matching).
///
/// Использует [`globset::Glob`] для сопоставления имён каналов, и позволяет
/// принимать сообщения из всех соответствующих каналов.
///
/// Отписка также происходит автоматически при `Drop`, либо явно через [`PatternSubscription::unsubscribe`].
pub struct PatternSubscription {
    /// Глобальный шаблон, используемый для сопоставления с именами каналов.
    pub pattern: Glob,
    /// Внутренний `broadcast::Receiver`, через который приходят сообщения.
    pub inner: broadcast::Receiver<Message>,
}

impl Subscription {
    /// Возвращает изменяемую ссылку на внутренний [`broadcast::Receiver`],
    /// с помощью которой можно вызывать `.recv().await`.
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Явная отписка от канала. Эквивалентна `drop(self)`.
    ///
    /// После вызова перестаёт получать сообщения.
    pub fn unsubscribe(self) {
        // ничего не нужно делать: при дропе inner Receiver убирается из broadcast
    }
}

impl PatternSubscription {
    /// Возвращает изменяемую ссылку на внутренний [`broadcast::Receiver`],
    /// с помощью которой можно вызывать `.recv().await`.
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Явная отписка от шаблонной подписки.
    ///
    /// После вызова перестаёт получать сообщения по соответствующему шаблону.
    pub fn unsubscribe(self) {
        // дропаем — broadcast уберёт Receiver
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use globset::Glob;
    use tokio::{sync::broadcast, time::timeout};

    use crate::{pubsub::PatternSubscription, Broker, Subscription};

    /// Проверяет, что подписка сохраняет правильное имя канала.
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

    /// Проверяет, что сообщение, опубликованное в канал, получено подписчиком.
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

    /// Проверяет, что при удалении подписки из канала, количество получателей
    /// уменьшается.
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

    /// Проверяет, что при явном вызове `unsubscribe` подписка удаляется
    /// корректно.
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

    /// Проверяет, что подписка по шаблону (`psubscribe`) корректно
    /// получает сообщения.
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

    /// Проверяет, что после отписки от шаблона, больше не приходит
    /// сообщений.
    #[tokio::test]
    async fn test_pattern_unsubscribe_stops_reception() {
        let broker = Broker::new(10);
        let mut psub = broker.psubscribe("bar*").unwrap();

        broker.punsubscribe("bar*").unwrap();

        broker.publish("barbaz", Bytes::from_static(b"nope"));

        // проверим, что канал закрыт, и ничего не приходит
        let result = timeout(Duration::from_millis(100), psub.receiver().recv()).await;

        assert!(
            result.is_err() || matches!(result.unwrap(), Err(broadcast::error::RecvError::Closed))
        );
    }

    /// Проверяет, что несколько подписок по шаблону получают одно
    /// и то же сообщение.
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

    /// Проверяет, что при удалении подписки по шаблону количество
    /// получателей уменьшается.
    #[test]
    fn test_pattern_unsubscribe_drops_receiver() {
        let (tx, rx) = broadcast::channel(3);
        let pattern = Glob::new("pat*").unwrap();

        let psub = PatternSubscription { pattern, inner: rx };
        assert_eq!(tx.receiver_count(), 1);
        drop(psub);
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Проверяет, что при явном вызове `unsubscribe` для шаблона
    /// подписка удаляется корректно.
    #[test]
    fn test_pattern_explicit_unsubscribe_consumes() {
        let (tx, rx) = broadcast::channel(3);
        let pattern = Glob::new("z*").unwrap();

        let psub = PatternSubscription { pattern, inner: rx };
        assert_eq!(tx.receiver_count(), 1);
        psub.unsubscribe();
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Проверяет, что при двух подписках на один и тот же канал
    /// оба получателя получат каждое опубликованное сообщение.
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

    /// Проверяет, что отписка от несуществующего канала или шаблона
    /// не приводит к панике и корректно возвращает управление.
    #[test]
    fn test_unsubscribe_nonexistent() {
        let broker = Broker::new(5);
        // оба должны просто вернуться без паники.
        broker.unsubscribe_all("nochan");
        broker.punsubscribe("no*pat").unwrap();
    }

    /// Проверяет, что при попытке подписаться по некорректному glob-шаблону
    /// возвращается ошибка парсинга шаблона.
    #[test]
    fn test_invalid_glob_pattern() {
        let broker = Broker::new(5);
        assert!(broker.psubscribe("[invalid[").is_err());
    }
}
