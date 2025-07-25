use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use bytes::Bytes;
use dashmap::DashMap;
use globset::Glob;
use tokio::sync::broadcast;

use super::{intern_channel, Message, PatternSubscription, Subscription};
use crate::RecvError;

type ChannelKey = Arc<str>;
type PatternKey = Glob;

/// Pub/Sub брокер сообщений.
///
/// Особенности:
/// - Подписка на точные каналы по имени
/// - Подписка по паттернам (glob-выражения)
/// - Автоматическое удаление пустых каналов
/// - Сбор статистики по публикациям и ошибкам отправки
#[derive(Debug)]
pub struct Broker {
    /// Точные каналы → отправители сообщений
    channels: Arc<DashMap<ChannelKey, broadcast::Sender<Message>>>,
    /// Паттерны (glob) → отправители сообщений
    patterns: Arc<DashMap<PatternKey, broadcast::Sender<Message>>>,
    /// Размер буфера для каждого канала
    default_capacity: usize,
    /// Максимальное количество подписчиков на канал (0 = без ограничений)
    max_subscribers_per_channel: usize,
    /// Общее число вызовов publish
    pub publish_count: AtomicUsize,
    /// Число ошибок отправки сообщений (когда нет подписчиков)
    pub send_error_count: AtomicUsize,
}

/// Статистика работы брокера.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerStats {
    /// Количество активных каналов.
    pub channel_count: usize,
    /// Количество активных паттернов.
    pub pattern_count: usize,
    /// Общее количество подписчиков.
    pub total_subscribers: usize,
    /// Общее количество публикаций.
    pub publish_count: usize,
    /// Количество ошибок отправки.
    pub send_error_count: usize,
}

impl Broker {
    /// Создаёт новый брокер с указанным размером буфера
    pub fn new(default_capacity: usize) -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
            patterns: Arc::new(DashMap::new()),
            default_capacity,
            max_subscribers_per_channel: 0,
            publish_count: AtomicUsize::new(0),
            send_error_count: AtomicUsize::new(0),
        }
    }

    /// Создаёт новый брокер с ограничением на количество подписчиков
    pub fn with_subscriber_limit(
        default_capacity: usize,
        max_subscribers: usize,
    ) -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
            patterns: Arc::new(DashMap::new()),
            default_capacity,
            max_subscribers_per_channel: max_subscribers,
            publish_count: AtomicUsize::new(0),
            send_error_count: AtomicUsize::new(0),
        }
    }
}

impl Broker {
    /// Подписка на паттерн (glob), например "kin.*" или "a?c".
    ///
    /// Повторная подписка на один и тот же паттерн возвращает
    /// тот же канал отправки.
    pub fn psubscribe(
        &self,
        pattern: &str,
    ) -> Result<PatternSubscription, RecvError> {
        let glob = Glob::new(pattern).map_err(RecvError::from)?;

        let tx = self
            .patterns
            .entry(glob.clone())
            .or_insert_with(|| broadcast::channel(self.default_capacity).0)
            .clone();

        // Проверяем лимит подписчиков, если он установлен.
        if self.max_subscribers_per_channel > 0
            && tx.receiver_count() >= self.max_subscribers_per_channel
        {
            return Err(RecvError::SubscriberLimitExceeded);
        }

        Ok(PatternSubscription {
            pattern: glob,
            inner: tx.subscribe(),
        })
    }

    /// Отписка от паттерна. Удаляет связанный отправитель.
    pub fn punsubscribe(
        &self,
        pattern: &str,
    ) -> Result<(), RecvError> {
        let glob = Glob::new(pattern).map_err(RecvError::from)?;
        self.patterns.remove(&glob);
        Ok(())
    }

    /// Подписка на точный канал по имени.
    ///
    /// Ключ канала — interned `Arc<str>`.
    pub fn subscribe(
        &self,
        channel: &str,
    ) -> Result<Subscription, RecvError> {
        let key: Arc<str> = intern_channel(channel);

        let tx = self
            .channels
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(self.default_capacity).0)
            .clone();

        // Проверяем лимит подписчиков, если он установлен.
        if self.max_subscribers_per_channel > 0
            && tx.receiver_count() >= self.max_subscribers_per_channel
        {
            return Err(RecvError::SubscriberLimitExceeded);
        }

        Ok(Subscription {
            channel: key,
            inner: tx.subscribe(),
        })
    }

    /// Подписка на точный канал по имени (convenience метод без обработки ошибок)
    ///
    /// Для обратной совместимости. Использует панику при превышении лимитов.
    pub fn subscribe_unchecked(
        &self,
        channel: &str,
    ) -> Subscription {
        self.subscribe(channel).unwrap_or_else(|e| {
            panic!("Failed to create a subscription to channel '{channel}': {e}")
        })
    }

    /// Публикация сообщения в канал.
    ///
    /// Работает в два этапа:
    /// 1. Отправка в точный канал (если есть)
    /// 2. Отправка всем подписчикам по подходящим паттернам
    ///
    /// Если в точном канале нет подписчиков, увеличивается
    /// счётчик ошибок, и канал удаляется.
    ///
    /// # Возвращает
    /// - `Ok(subscriber_count)` - количество подписчиков, получивших сообщение
    /// - `Err(RecvError::ChannelNotFound)` - если канал не существует и паттерны не совпали
    pub fn publish(
        &self,
        channel: &str,
        payload: Bytes,
    ) -> Result<usize, RecvError> {
        self.publish_count.fetch_add(1, Ordering::Relaxed);
        let mut total_subscribers = 0;
        let mut found_any_channel = false;

        if let Some(entry) = self.channels.get_mut(channel) {
            found_any_channel = true;
            let tx = entry.value().clone();
            let msg = Message::new(entry.key().clone(), payload.clone());

            let subscriber_count = tx.receiver_count();
            total_subscribers += subscriber_count;

            if tx.send(msg).is_err() {
                self.send_error_count.fetch_add(1, Ordering::Relaxed);
            }

            // Если подписчиков нет, удаляем канал
            if tx.receiver_count() == 0 {
                let key = entry.key().clone();
                drop(entry);
                self.channels.remove(&*key);
            }
        }

        for entry in self.patterns.iter() {
            let matcher = entry.key().compile_matcher();
            if matcher.is_match(channel) {
                found_any_channel = true;
                let tx = entry.value().clone();
                let msg = Message::new(channel, payload.clone());

                let subscriber_count = tx.receiver_count();
                total_subscribers += subscriber_count;

                if tx.send(msg).is_err() {
                    self.send_error_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if found_any_channel {
            Ok(total_subscribers)
        } else {
            Err(RecvError::ChannelNotFound(channel.to_string()))
        }
    }

    /// Публикация сообщения в канал (convenience метод без обработки ошибок).
    ///
    /// Для обратной совместимости с существующим API.
    pub fn publish_unchecked(
        &self,
        channel: &str,
        payload: Bytes,
    ) {
        let _ = self.publish(channel, payload);
    }

    /// Проверяет, существует ли канал (есть ли подписчик)
    pub fn channel_exists(
        &self,
        channel: &str,
    ) -> bool {
        self.channels
            .get(channel)
            .map(|entry| entry.receiver_count() > 0)
            .unwrap_or(false)
    }

    /// Проверяет, подходит ли канал под какой-либо из активных паттернов.
    pub fn matches_any_pattern(
        &self,
        channel: &str,
    ) -> bool {
        self.patterns.iter().any(|entry| {
            let matcher = entry.key().compile_matcher();
            matcher.is_match(channel)
        })
    }

    /// Удаляет все подписки и сам канал.
    ///
    /// После этого публикации в канал не создадут его заново.
    pub fn unsubscribe_all(
        &self,
        channel: &str,
    ) {
        self.channels.remove(channel);
    }

    /// Возвращает количество активных подписок на точные каналы.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Возвращает количество активных подписок по паттернам.
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// Возвращает список имён всех активных каналов.
    pub fn active_channels(&self) -> Vec<Arc<str>> {
        self.channels
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Возвращает список всех активных паттернов.
    pub fn active_patterns(&self) -> Vec<String> {
        self.patterns
            .iter()
            .map(|entry| entry.key().glob().to_string())
            .collect()
    }

    /// Возвращает количество подписчиков на конкретный канал.
    pub fn subscriber_count(
        &self,
        channel: &str,
    ) -> usize {
        self.channels
            .get(channel)
            .map(|entry| entry.receiver_count())
            .unwrap_or(0)
    }

    /// Возвращает количество подписчиков на конкретный паттерн.
    pub fn pattern_subscriber_count(
        &self,
        pattern: &str,
    ) -> Result<usize, RecvError> {
        let glob = Glob::new(pattern).map_err(RecvError::from)?;
        Ok(self
            .patterns
            .get(&glob)
            .map(|entry| entry.receiver_count())
            .unwrap_or(0))
    }

    /// Возвращает общуюю статистику брокера.
    pub fn stats(&self) -> BrokerStats {
        let total_subscribers: usize = self
            .channels
            .iter()
            .map(|entry| entry.receiver_count())
            .sum::<usize>()
            + self
                .patterns
                .iter()
                .map(|entry| entry.receiver_count())
                .sum::<usize>();

        BrokerStats {
            channel_count: self.channel_count(),
            pattern_count: self.pattern_count(),
            total_subscribers,
            publish_count: self.publish_count.load(Ordering::Relaxed),
            send_error_count: self.send_error_count.load(Ordering::Relaxed),
        }
    }

    /// Очищает все каналы и паттерны.
    pub fn clear(&self) {
        self.channels.clear();
        self.patterns.clear();
    }

    /// Устанавливает лимит подписчиков на канал.
    pub fn set_subscriber_limit(
        &mut self,
        limit: usize,
    ) {
        self.max_subscribers_per_channel = limit;
    }

    /// Возвращает текущий лимит подписчиков на канал.
    pub fn subscriber_limit(&self) -> usize {
        self.max_subscribers_per_channel
    }
}

impl BrokerStats {
    /// Возвращает успешность доставки сообщений как процент.
    pub fn delivery_success_rate(&self) -> f64 {
        if self.publish_count == 0 {
            return 100.0;
        }
        let successful = self.publish_count.saturating_sub(self.send_error_count);
        (successful as f64 / self.publish_count as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use tokio::{
        sync::broadcast::error::RecvError,
        time::{timeout, Duration},
    };

    use crate::{RecvError as BroadcastRecvError, TryRecvError};

    use super::*;

    /// Helper: создает брокера и подписывается на него, возвращая
    /// (брокеру, получателю)
    async fn setup_one() -> (Broker, tokio::sync::broadcast::Receiver<Message>) {
        let broker = Broker::new(5);
        // Распаковываем результат подписки
        let subscription = broker
            .subscribe("chan")
            .unwrap_or_else(|e| panic!("failed to subscribe: {e:?}"));

        // Достаем из Subscription его внутренний Receiver
        let rx = subscription.inner;

        (broker, rx)
    }

    /// Тест проверяет, что сообщение доставляется подписчику,
    /// и что счётчики публикаций и ошибок обновляются корректно.
    #[tokio::test]
    async fn test_publish_and_receive() {
        let (broker, mut rx) = setup_one().await;
        broker.publish("chan", Bytes::from_static(b"x")).unwrap();
        let msg = timeout(Duration::from_millis(50), rx.recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg.channel, "chan");
        assert_eq!(msg.payload, Bytes::from_static(b"x"));
        // publish_count должно быть равно 1, send_error_count == 0
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
    }

    /// Тест проверяет, что публикация в несуществующий канал
    /// не создаёт канал и не увеличивает счётчик ошибок.
    #[tokio::test]
    async fn test_publish_to_nonexistent_channel() {
        let broker = Broker::new(5);
        broker.publish_unchecked("nochan", Bytes::from_static(b"z"));
        // Подписчиков нет, канал не создан, значение send_error не увеличивается
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("nochan"));
    }

    /// Тест проверяет, что все подписчики канала получают
    /// сообщение.
    #[tokio::test]
    async fn test_multiple_subscribers_receive() {
        let broker = Broker::new(5);
        // Создаём три подписчика и сразу получаем их внутренние Receivers
        let subs: Vec<_> = (0..3)
            .map(|_| {
                // Распаковываем Result<Subscription, _> → Subscription
                let subscription = broker
                    .subscribe("multi")
                    .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
                // Берём из него inner: broadcast::Receiver<Message>
                subscription.inner
            })
            .collect();

        // Публикуем одно сообщение
        broker.publish("multi", Bytes::from_static(b"d")).unwrap();

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

    /// Тест проверяет, что если подписка сброшена и никто не
    /// слушает, публикация увеличивает счётчик ошибок, а канал
    /// удаляется.
    #[tokio::test]
    async fn test_auto_remove_empty_channel_and_error_count() {
        // 1) подпишитесь и немедленно прекратите подписку
        let broker = Broker::new(5);
        {
            let sub = broker
                .subscribe("temp")
                .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
            drop(sub);
        }
        // канал все еще существует до первой публикации
        assert!(broker.channels.contains_key("temp"));

        // 2) публикация должна вызвать ошибку send_error и удалить канал
        broker.publish("temp", Bytes::from_static(b"u")).unwrap();
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 1);
        assert!(!broker.channels.contains_key("temp"));
    }

    /// Тест проверяет, что после `unsubscribe_all` публикации
    /// игнорируются.
    #[tokio::test]
    async fn test_unsubscribe_all() {
        let broker = Broker::new(5);
        let _sub = broker
            .subscribe("gone")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        // теперь удалите все подписки
        broker.unsubscribe_all("gone");
        assert!(!broker.channels.contains_key("gone"));

        // при публикации после удаления увеличивается значение publish_count,
        // но не значение send_error_count, и канал не создается заново
        broker.publish_unchecked("gone", Bytes::from_static(b"x"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("gone"));
    }

    /// Тест проверяет, что подписки по паттернам (psubscribe)
    /// получают сообщения.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_psubscribe_and_receive() {
        let broker = Broker::new(5);
        let mut psub = broker
            .psubscribe("foo.*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        // точный канал также соответствует шаблону
        broker.publish("foo.bar", Bytes::from_static(b"X")).unwrap();
        let msg = psub.receiver().recv().await.expect("no msg");
        assert_eq!(&*msg.channel, "foo.bar");
        assert_eq!(msg.payload, Bytes::from_static(b"X"));
    }

    /// Тест проверяет, что обычные и паттерн-подписки работают
    /// вместе.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_sub_and_psub_together() {
        let broker = Broker::new(5);

        // Распаковываем результаты подписок
        let mut sub = broker
            .subscribe("topic")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let mut psub = broker
            .psubscribe("t*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Публикуем сообщение
        broker.publish("topic", Bytes::from_static(b"Z")).unwrap();

        // Получаем из каждого подписчика через receiver()
        let m1 = sub.receiver().recv().await.expect("no exact");
        let m2 = psub.receiver().recv().await.expect("no pattern");

        // Проверяем, что оба подписчика получили одну и ту же вещь
        assert_eq!(&*m1.channel, "topic");
        assert_eq!(&*m2.channel, "topic");
        assert_eq!(m1.payload, Bytes::from_static(b"Z"));
        assert_eq!(m2.payload, Bytes::from_static(b"Z"));
    }

    /// Тест проверяет, что после `punsubscribe` приёмник
    /// закрывается.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_punsubscribe_no_receive() {
        let broker = Broker::new(5);
        let mut psub = broker
            .psubscribe("a?c")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        // удалить шаблон у брокера
        broker.punsubscribe("a?c").unwrap();
        // отправителей больше нет, получатель должен быть закрыт
        let res = psub.receiver().recv().await;
        use tokio::sync::broadcast::error::RecvError;
        assert!(matches!(res, Err(RecvError::Closed)));
    }

    /// Тест проверяет, что два подписчика на один канал
    /// оба получают каждое сообщение.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_multiple_subscribe_same_channel() {
        let broker = Broker::new(5);

        let mut sub1 = broker
            .subscribe("dup")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let mut sub2 = broker
            .subscribe("dup")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        let rx1 = sub1.receiver();
        let rx2 = sub2.receiver();

        broker.publish("dup", Bytes::from_static(b"hi")).unwrap();

        let msg1 = rx1.recv().await.unwrap();
        let msg2 = rx2.recv().await.unwrap();

        assert_eq!(&*msg1.channel, "dup");
        assert_eq!(&*msg2.channel, "dup");
        assert_eq!(msg1.payload, Bytes::from_static(b"hi"));
        assert_eq!(msg2.payload, Bytes::from_static(b"hi"));
    }

    /// Тест проверяет, что при дропе подписки
    /// количество подписчиков уменьшается.
    #[tokio::test]
    async fn test_drop_subscription_decrements_receiver_count() {
        let broker = Broker::new(5);
        let sub = broker
            .subscribe("tmp")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let key = Arc::clone(&sub.channel);
        let sender = broker.channels.get(&*key).unwrap().clone();
        assert_eq!(sender.receiver_count(), 1);
        drop(sub);
        // Дайте время капле распространиться
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(sender.receiver_count(), 0);
    }

    /// Тест проверяет поведение при переполнении буфера:
    /// старое сообщение удаляется, recv() возвращает `Lagged`.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_broadcast_overwrites_when_buffer_full() {
        let broker = Broker::new(1); // размер буфера = 1

        // Сохраняйте подписку, чтобы она не была удалена
        let mut subscription = broker
            .subscribe("overflow")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let sub = subscription.receiver();

        // Отправить первое сообщение
        broker
            .publish("overflow", Bytes::from_static(b"first"))
            .unwrap();
        // Отправьте второе сообщение — оно должно удалить первое
        broker
            .publish("overflow", Bytes::from_static(b"second"))
            .unwrap();

        // Получение должно привести к ошибке (задержка(1)) из-за потери сообщения
        let err = sub.recv().await.unwrap_err();
        assert!(
            matches!(err, RecvError::Lagged(1)),
            "Expected Lagged(1), got: {err:?}"
        );
    }

    /// Тест проверяет, что psubscribe возвращает ошибку на
    /// неправильный паттерн.
    #[tokio::test]
    async fn test_psubscribe_invalid_pattern() {
        let broker = Broker::new(5);
        let res = broker.psubscribe("[invalid");
        assert!(res.is_err());
    }

    /// Тест проверяет, что после `unsubscribe_all` канал не
    /// создаётся заново, а статистика обновляется правильно.
    #[tokio::test]
    async fn test_publish_after_unsubscribe_all_does_not_create_channel() {
        let broker = Broker::new(5);
        let _ = broker
            .subscribe("vanish")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        broker.unsubscribe_all("vanish");
        assert!(!broker.channels.contains_key("vanish"));

        broker.publish_unchecked("vanish", Bytes::from_static(b"y"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("vanish")); // канал не следует создавать заново
    }

    /// Тест проверяет асинхронную доставку одного сообщения через `recv()`.
    #[tokio::test]
    async fn test_subscription_recv_success() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("test_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Публикуем сообщение
        broker
            .publish("test_channel", Bytes::from("hello world"))
            .unwrap();

        // Получаем сообщение асинхронно
        let result = sub.recv().await;
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.channel.as_ref(), "test_channel");
        assert_eq!(msg.payload, Bytes::from("hello world"));
    }

    /// Тест проверяет немедленное получение через `try_recv().await`.
    #[tokio::test]
    async fn test_subscription_try_recv_success() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("test_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Публикуем сообщение.
        broker
            .publish("test_channel", Bytes::from("immediate"))
            .unwrap();

        // Пытаемся получить сообщение немедленно.
        let result = sub.try_recv();
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.channel.as_ref(), "test_channel");
        assert_eq!(msg.payload, Bytes::from("immediate"));
    }

    /// Тест проверяет, что `try_recv().await` возвращает `Empty` на пустом канале.
    #[tokio::test]
    async fn test_subscription_try_recv_empty() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("empty_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Пытаемся получить сообщение из пустого канала.
        let result = sub.try_recv();
        assert!(matches!(result, Err(TryRecvError::Empty)));
    }

    /// Тест проверяет получение одного сообщения по шаблону через `recv()`.
    #[tokio::test]
    async fn test_pattern_subscription_recv_success() -> Result<(), RecvError> {
        let broker = Broker::new(10);
        let mut pattern_sub = broker
            .psubscribe("test.*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Публикуем в канал, соответствующий паттерну.
        broker
            .publish("test.foo", Bytes::from("pattern message"))
            .unwrap();

        // Получаем сообщение.
        let result = pattern_sub.recv().await;
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.channel.as_ref(), "test.foo");
        assert_eq!(msg.payload, Bytes::from("pattern message"));

        Ok(())
    }

    /// Тест проверяет получение нескольких сообщений по одному шаблону.
    #[tokio::test]
    async fn test_pattern_subscription_multiple_channels() -> Result<(), RecvError> {
        let broker = Broker::new(10);
        let mut pattern_sub = broker
            .psubscribe("user.*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Публикуем в несколько каналов, соответствующих паттерну.
        broker
            .publish("user.login", Bytes::from("user login"))
            .unwrap();
        broker
            .publish("user.logout", Bytes::from("user logout"))
            .unwrap();

        // Получаем первое сообщение.
        let msg1 = pattern_sub.recv().await.unwrap();
        assert_eq!(msg1.channel.as_ref(), "user.login");

        // Получаем второе сообщение.
        let msg2 = pattern_sub.recv().await.unwrap();
        assert_eq!(msg2.channel.as_ref(), "user.logout");

        Ok(())
    }

    /// Тест проверяет, что `recv()` на закрытом канале возвращает `Closed`.
    #[tokio::test]
    async fn test_subscription_closed_error() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("closing_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Удаляем все подписки, что должно закрыть канал.
        broker.unsubscribe_all("closing_channel");

        // Пытаемся получить сообщение из закрытого канала.
        let result = timeout(Duration::from_millis(100), sub.recv()).await;

        // Проверяем, что получили ошибку о закрытии канала.
        if let Ok(recv_result) = result {
            assert!(matches!(recv_result, Err(BroadcastRecvError::Closed)));
        }
    }

    /// Тест проверяет получение нескольких сообщений подряд через `recv()`.
    #[tokio::test]
    async fn test_subscription_multiple_messages() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("multi_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Публикуем несколько сообщений.
        for i in 0..5 {
            broker
                .publish("multi_channel", Bytes::from(format!("message: {i}")))
                .unwrap();
        }

        // Получаем все сообщения.
        for i in 0..5 {
            let result = sub.recv().await;
            assert!(result.is_ok());

            let msg = result.unwrap();
            assert_eq!(msg.payload, Bytes::from(format!("message: {i}")));
        }
    }

    /// Тест проверяет параллельную доставку одному сообщению нескольким подписчикам.
    #[tokio::test]
    async fn test_concurrent_subscriptions() {
        let broker = Arc::new(Broker::new(10));
        let mut handles = Vec::new();

        // Создаём несколько подписчиков
        for i in 0..3 {
            let broker_clone = broker.clone();
            let handle = tokio::spawn(async move {
                let mut sub = broker_clone
                    .subscribe("concurrent_channel")
                    .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
                let msg = sub.recv().await.unwrap();
                format!(
                    "subscriber-{}: {}",
                    i,
                    String::from_utf8_lossy(&msg.payload)
                )
            });
            handles.push(handle);
        }

        // Даём подписчикам время подписаться
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Публикуем сообщение
        broker
            .publish("concurrent_channel", Bytes::from("broadcast"))
            .unwrap();

        // Ждём результатов
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // Проверяем, что все получили сообщение
        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.contains("broadcast"));
        }
    }

    /// Тест проверяет таймаут при отсутствии сообщений (`timeout`).
    #[tokio::test]
    async fn test_subscription_timeout() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("timeout_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Пытаемся получить сообщение с таймаутом
        let result = timeout(Duration::from_millis(50), sub.recv()).await;

        // Проверяем, что произошел таймаут
        assert!(result.is_err());
    }

    /// Тест проверяет вспомогательные методы подписки: `len`, `is_empty`, `channel_name`, `is_closed`.
    #[tokio::test]
    async fn test_subscription_helper_methods() {
        let broker = Broker::new(10);
        let mut sub = broker
            .subscribe("helper_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Проверяем helper методы
        assert_eq!(sub.channel_name().as_ref(), "helper_channel");
        assert!(!sub.is_closed());
        assert_eq!(sub.len(), 0);
        assert!(sub.is_empty());

        // Публикуем сообщение
        broker
            .publish("helper_channel", Bytes::from("test"))
            .unwrap();

        // Проверяем, что очередь не пуста
        assert_eq!(sub.len(), 1);
        assert!(!sub.is_empty());

        // Получаем сообщение
        let _ = sub.recv().await.unwrap();
        assert_eq!(sub.len(), 0);
        assert!(sub.is_empty());
    }

    /// Тест проверяет вспомогательные методы паттерн-подписки: `len`, `is_empty`, `is_closed`.
    #[tokio::test]
    async fn test_pattern_subscription_helper_methods() -> Result<(), RecvError> {
        let broker = Broker::new(10);
        let pattern_sub = broker.psubscribe("helper.*").unwrap();

        // Проверяем helper методы
        assert!(!pattern_sub.is_closed());
        assert_eq!(pattern_sub.len(), 0);
        assert!(pattern_sub.is_empty());

        // Публикуем сообщение
        broker.publish("helper.test", Bytes::from("test")).unwrap();

        // Проверяем, что очередь не пуста
        assert_eq!(pattern_sub.len(), 1);
        assert!(!pattern_sub.is_empty());

        Ok(())
    }

    /// Тест проверяет форматирование ошибок типов `RecvError` и `TryRecvError`.
    #[tokio::test]
    async fn test_error_types_display() {
        // Тестируем форматирование ошибок
        let recv_closed = BroadcastRecvError::Closed;
        let recv_lagged = BroadcastRecvError::Lagged(42);

        assert_eq!(recv_closed.to_string(), "channel is closed");
        assert_eq!(
            recv_lagged.to_string(),
            "receiver lagged behind by 42 messages"
        );

        let try_recv_empty = TryRecvError::Empty;
        let try_recv_closed = TryRecvError::Closed;
        let try_recv_lagged = TryRecvError::Lagged(10);

        assert_eq!(try_recv_empty.to_string(), "no messages available");
        assert_eq!(try_recv_closed.to_string(), "channel is closed");
        assert_eq!(
            try_recv_lagged.to_string(),
            "receiver lagged behind by 10 messages"
        );
    }

    /// Тест проверяет сценарий переполнения буфера и генерацию `Lagged` ошибки.
    #[tokio::test]
    async fn test_lagged_error_scenario() {
        let broker = Broker::new(2); // Маленький буфер для провокации lag
        let mut sub = broker
            .subscribe("lag_channel")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Публикуем много сообщений, чтобы переполнить буфер
        for i in 0..10 {
            broker
                .publish("lag_channel", Bytes::from(format!("msg{i}")))
                .unwrap();
        }

        // Первое получение может вернуть Lagged ошибку
        match sub.recv().await {
            Ok(_) => {} // Иногда получаем сообщение
            Err(BroadcastRecvError::Lagged(n)) => {
                assert!(n > 0);
                println!("Successfully caught lagged error: {n} messages");
            }
            Err(e) => panic!("Unexpected error: {e}"),
        }
    }

    #[tokio::test]
    async fn test_broker_with_subscriber_limit() {
        let broker = Broker::with_subscriber_limit(10, 2);

        // Первые две подписки должны пройти.
        let _sub1 = broker
            .subscribe("test")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let _sub2 = broker
            .subscribe("test")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        // Третья должна вернуть оишбку.
        let result = broker.subscribe("test");
        assert!(matches!(
            result,
            Err(BroadcastRecvError::SubscriberLimitExceeded)
        ));
    }

    #[tokio::test]
    async fn test_publish_with_error_handling() {
        let broker = Broker::new(10);

        // Публикация в несуществующий канал
        let result = broker.publish("nonexistent", Bytes::from("test"));
        assert!(matches!(
            result,
            Err(BroadcastRecvError::ChannelNotFound(_))
        ));

        // Создаем подписку и публикуем
        let _sub = broker
            .subscribe("test")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let result = broker.publish("test", Bytes::from("test")).unwrap();
        assert_eq!(result, 1); // Один подписчик
    }

    #[tokio::test]
    async fn test_pattern_subscription_errors() {
        let broker = Broker::new(10);

        // Некорректный glob-паттерн
        let result = broker.psubscribe("[invalid");
        assert!(matches!(result, Err(BroadcastRecvError::InvalidPattern(_))));

        // Корректный паттерн
        let result = broker.psubscribe("test.*");
        assert!(result.is_ok());
    }

    #[test]
    fn test_broker_stats() {
        let broker = Broker::new(10);
        let stats = broker.stats();

        assert_eq!(stats.channel_count, 0);
        assert_eq!(stats.pattern_count, 0);
        assert_eq!(stats.total_subscribers, 0);
        assert_eq!(stats.delivery_success_rate(), 100.0);
    }

    #[test]
    fn test_channel_existence_checks() {
        let broker = Broker::new(10);

        assert!(!broker.channel_exists("test"));
        assert!(!broker.matches_any_pattern("test"));

        let _sub = broker
            .subscribe("test")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        assert!(broker.channel_exists("test"));

        let _pattern_sub = broker
            .psubscribe("test.*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        assert!(broker.matches_any_pattern("test.foo"));
    }

    #[test]
    fn test_active_channels_and_patterns() {
        let broker = Broker::new(10);

        let _sub1 = broker
            .subscribe("channel1")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let _sub2 = broker
            .subscribe("channel2")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
        let _pattern_sub = broker
            .psubscribe("pattern.*")
            .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

        let channels = broker.active_channels();
        assert_eq!(channels.len(), 2);

        let patterns = broker.active_patterns();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0], "pattern.*");
    }
}
