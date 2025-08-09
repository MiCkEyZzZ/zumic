use crate::{Broker, MessagePayload, RecvError, Subscriber};

/// Менеджер Pub/Sub-системы, хранится внутри StorageEngine.
#[derive(Debug)]
pub struct PubSubManager {
    broker: Broker,
}

impl PubSubManager {
    /// Создаёт новый PubSubManager.
    pub fn new() -> Self {
        Self {
            broker: Broker::new(),
        }
    }

    /// Публикация байтов в канал.
    pub fn publish_bytes<S: AsRef<str>>(
        &self,
        channel: S,
        payload: bytes::Bytes,
    ) -> Result<usize, RecvError> {
        let result = self
            .broker
            .publish(channel, MessagePayload::Bytes(payload))?;
        Ok(result.subscribers_reached)
    }

    /// Публикация строки в канал.
    pub fn publish_string<S: AsRef<str>, T: Into<String>>(
        &self,
        channel: S,
        message: T,
    ) -> Result<usize, RecvError> {
        let result = self.broker.publish_str(channel, message)?;
        Ok(result.subscribers_reached)
    }

    /// Публикация JSON в канал.
    pub fn publish_json<S: AsRef<str>, T: serde::Serialize>(
        &self,
        channel: S,
        value: &T,
    ) -> Result<usize, RecvError> {
        let result = self.broker.publish_json(channel, value)?;
        Ok(result.subscribers_reached)
    }

    /// Подписка на точный канал.
    pub fn subscribe<S: AsRef<str>>(
        &self,
        channel: S,
    ) -> Result<Subscriber, RecvError> {
        self.broker.subscribe(channel)
    }

    /// Подписка на точный канал с опциями.
    pub fn subscribe_with_options<S: AsRef<str>>(
        &self,
        channel: S,
        options: crate::pubsub::SubscriptionOptions,
    ) -> Result<Subscriber, RecvError> {
        self.broker.subscribe_with_options(channel, options)
    }

    /// Отписаться от канала (закрывает канал для всех подписчиков).
    pub fn unsubscribe(
        &self,
        channel: &str,
    ) -> bool {
        self.broker.unsubscribe(channel)
    }

    /// Возвращает количество подписчиков на канал.
    pub fn subscriber_count<S: AsRef<str>>(
        &self,
        channel: S,
    ) -> usize {
        self.broker.subscriber_count(channel)
    }

    /// Возвращает список всех активных каналов.
    pub fn active_channels(&self) -> Vec<String> {
        self.broker.active_channels()
    }

    /// Возвращает статистику по каналу.
    pub fn channel_stats<S: AsRef<str>>(
        &self,
        channel: S,
    ) -> Option<crate::pubsub::ChannelStats> {
        self.broker.channel_stats(channel)
    }

    /// Возвращает глобальные метрики брокера.
    pub fn metrics(&self) -> crate::pubsub::BrokerMetrics {
        self.broker.metrics()
    }

    /// Очищает неактивные каналы.
    pub fn cleanup_inactive_channels(&self) -> usize {
        self.broker.cleanup_inactive_channels()
    }

    /// Создаёт снимок состояния брокера.
    pub fn snapshot(&self) -> crate::pubsub::BrokerSnapshot {
        self.broker.snapshot()
    }

    /// Устаревший метод для обратной совместимости.
    /// Используйте publish_bytes вместо этого.
    #[deprecated(since = "0.1.0", note = "Use publish_bytes instead")]
    pub fn publish(
        &self,
        channel: &str,
        payload: bytes::Bytes,
    ) -> Result<usize, RecvError> {
        self.publish_bytes(channel, payload)
    }

    /// Устаревший метод для обратной совместимости.
    /// Используйте unsubscribe вместо этого.
    #[deprecated(since = "0.1.0", note = "Use unsubscribe instead")]
    pub fn unsubscribe_all(
        &self,
        channel: &str,
    ) {
        self.unsubscribe(channel);
    }

    /// Устаревший метод для обратной совместимости.
    /// Используйте metrics вместо этого.
    #[deprecated(since = "0.1.0", note = "Use metrics instead")]
    pub fn stats(&self) -> (usize, usize) {
        let metrics = self.broker.metrics();
        (
            metrics
                .total_messages
                .load(std::sync::atomic::Ordering::Relaxed) as usize,
            0, // send_error_count не поддерживается в новой архитектуре
        )
    }
}

impl Default for PubSubManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use serde_json::json;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Тест проверяет, что публикация в канал доставляется подписчику.
    #[tokio::test]
    async fn test_publish_and_subscribe() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("news").unwrap();

        manager.publish_bytes("news", Bytes::from("hello")).unwrap();

        let msg = timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout")
            .expect("no message");

        if let crate::pubsub::MessagePayload::Bytes(payload) = msg.payload {
            assert_eq!(payload, Bytes::from("hello"));
        } else {
            panic!("Expected Bytes payload");
        }
        assert_eq!(msg.channel.as_ref(), "news");
    }

    /// Тест проверяет публикацию строки.
    #[tokio::test]
    async fn test_publish_string() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("chat").unwrap();

        manager.publish_string("chat", "Hello, World!").unwrap();

        let msg = timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout")
            .expect("no message");

        if let crate::pubsub::MessagePayload::String(content) = msg.payload {
            assert_eq!(content, "Hello, World!");
        } else {
            panic!("Expected String payload");
        }
    }

    /// Тест проверяет публикацию JSON.
    #[tokio::test]
    async fn test_publish_json() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("events").unwrap();

        let data = json!({
            "event": "user_login",
            "user_id": 123
        });

        manager.publish_json("events", &data).unwrap();

        let msg = timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout")
            .expect("no message");

        if let crate::pubsub::MessagePayload::Json(json_data) = msg.payload {
            assert_eq!(json_data["event"], "user_login");
            assert_eq!(json_data["user_id"], 123);
        } else {
            panic!("Expected Json payload");
        }
    }

    /// Тест проверяет, что несколько подписчиков получают сообщение.
    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = PubSubManager::new();
        let mut sub1 = manager.subscribe("broadcast").unwrap();
        let mut sub2 = manager.subscribe("broadcast").unwrap();

        let subscriber_count = manager.subscriber_count("broadcast");
        assert_eq!(subscriber_count, 2);

        manager.publish_string("broadcast", "Hello all!").unwrap();

        let msg1 = timeout(Duration::from_millis(100), sub1.recv())
            .await
            .expect("timeout")
            .expect("no message");

        let msg2 = timeout(Duration::from_millis(100), sub2.recv())
            .await
            .expect("timeout")
            .expect("no message");

        // Оба подписчика должны получить одинаковое сообщение
        assert_eq!(msg1.channel, msg2.channel);
        assert_eq!(msg1.payload, msg2.payload);
    }

    /// Тест проверяет отписку от канала.
    #[tokio::test]
    async fn test_unsubscribe() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("temp").unwrap();

        assert_eq!(manager.subscriber_count("temp"), 1);

        // Отписываемся
        assert!(manager.unsubscribe("temp"));

        // Публикуем сообщение
        manager
            .publish_string("temp", "should not receive")
            .unwrap();

        // Подписчик должен получить ошибку о закрытом канале
        match timeout(Duration::from_millis(100), sub.recv()).await {
            Ok(Err(crate::RecvError::Closed)) => {
                // Ожидаемое поведение
            }
            Ok(Ok(_)) => panic!("Should not receive message after unsubscribe"),
            Err(_) => {
                // Таймаут тоже допустим, так как канал может быть просто закрыт
            }
            Ok(Err(e)) => panic!("Unexpected error: {e:?}"),
        }
    }

    /// Тест проверяет работу с активными каналами.
    #[tokio::test]
    async fn test_active_channels() {
        let manager = PubSubManager::new();

        assert_eq!(manager.active_channels().len(), 0);

        let _sub1 = manager.subscribe("channel1").unwrap();
        let _sub2 = manager.subscribe("channel2").unwrap();

        let active = manager.active_channels();
        assert_eq!(active.len(), 2);
        assert!(active.contains(&"channel1".to_string()));
        assert!(active.contains(&"channel2".to_string()));
    }

    /// Тест проверяет получение статистики по каналу.
    #[tokio::test]
    async fn test_channel_stats() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("stats_test").unwrap();

        // Изначально статистика должна существовать
        let stats = manager.channel_stats("stats_test");
        assert!(stats.is_some());

        // Публикуем сообщение
        manager.publish_string("stats_test", "test").unwrap();
        let _msg = sub.recv().await.unwrap();

        // Статистика должна обновиться
        let updated_stats = manager.channel_stats("stats_test").unwrap();
        assert!(updated_stats.messages_sent > 0);
    }

    /// Тест проверяет глобальные метрики.
    #[test]
    fn test_global_metrics() {
        let manager = PubSubManager::new();
        let _sub = manager.subscribe("metrics_test").unwrap();

        manager.publish_string("metrics_test", "test").unwrap();

        let metrics = manager.metrics();
        assert!(
            metrics
                .total_messages
                .load(std::sync::atomic::Ordering::Relaxed)
                > 0
        );
    }

    /// Тест проверяет обратную совместимость устаревших методов.
    #[tokio::test]
    async fn test_deprecated_methods() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("deprecated").unwrap();

        // Устаревший метод publish
        #[allow(deprecated)]
        manager.publish("deprecated", Bytes::from("test")).unwrap();

        let msg = sub.recv().await.unwrap();
        if let crate::pubsub::MessagePayload::Bytes(payload) = msg.payload {
            assert_eq!(payload, Bytes::from("test"));
        }

        // Устаревший метод stats
        #[allow(deprecated)]
        let (messages, errors) = manager.stats();
        assert!(messages > 0);
        assert_eq!(errors, 0); // всегда 0 в новой архитектуре

        // Устаревший метод unsubscribe_all
        #[allow(deprecated)]
        manager.unsubscribe_all("deprecated");
    }

    /// Тест проверяет реализацию Default.
    #[test]
    fn test_default_impl() {
        let default_manager = PubSubManager::default();
        let new_manager = PubSubManager::new();

        // Оба должны иметь одинаковые начальные метрики
        let default_metrics = default_manager.metrics();
        let new_metrics = new_manager.metrics();

        assert_eq!(
            default_metrics
                .total_messages
                .load(std::sync::atomic::Ordering::Relaxed),
            new_metrics
                .total_messages
                .load(std::sync::atomic::Ordering::Relaxed)
        );
    }
}
