use bytes::Bytes;
use globset::Error as GlobError;

use crate::{Broker, PatternSubscription, Subscription};

/// Менеджер Pub/Sub-системы, хранится внутри StorageEngine.
#[derive(Debug)]
pub struct PubSubManager {
    broker: Broker,
}

impl PubSubManager {
    /// Создаёт новый PubSubManager с буфером по умолячанию.
    pub fn new() -> Self {
        Self {
            broker: Broker::new(128),
        }
    }

    /// Публикация сообщения в канал.
    pub fn publish(
        &self,
        channel: &str,
        payload: Bytes,
    ) {
        self.broker.publish(channel, payload);
    }

    /// Подписка на точный канал.
    pub fn subscribe(
        &self,
        channel: &str,
    ) -> Subscription {
        self.broker.subscribe(channel)
    }

    /// Отписаться от всех подписок на канал.
    pub fn unsubscribe_all(
        &self,
        channel: &str,
    ) {
        self.broker.unsubscribe_all(channel);
    }

    /// Подписка по шаблону.
    pub fn psubscribe(
        &self,
        pattern: &str,
    ) -> Result<PatternSubscription, GlobError> {
        self.broker.psubscribe(pattern)
    }

    /// Отписаться от шаблона.
    pub fn punsubscribe(
        &self,
        pattern: &str,
    ) -> Result<(), GlobError> {
        self.broker.punsubscribe(pattern)
    }

    /// Доп. статистика (если нужно)
    pub fn stats(&self) -> (usize, usize) {
        (
            self.broker
                .publish_count
                .load(std::sync::atomic::Ordering::Relaxed),
            self.broker
                .send_error_count
                .load(std::sync::atomic::Ordering::Relaxed),
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

    /// Тест проверяет, что публикация в канал доставляется подписчику.
    #[tokio::test]
    async fn test_publish_and_subscribe() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("news");

        manager.publish("news", Bytes::from("hello"));

        let msg = sub.recv().await.unwrap();
        assert_eq!(msg.payload, Bytes::from("hello"));
        assert_eq!(&*msg.channel, "news");
    }

    /// Тест проверяет, что после unsubscribe_all подписчик не получает сообщений.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_unsubscribe_all() {
        let manager = PubSubManager::new();
        let mut sub = manager.subscribe("updates");

        manager.unsubscribe_all("updates");
        manager.publish("updates", Bytes::from("nope"));

        // Ждём максимум 100 мс, затем `unwrap()` по таймауту
        let res =
            tokio::time::timeout(std::time::Duration::from_millis(100), sub.receiver().recv())
                .await
                .expect("recv() future timed out itself");
        // И теперь ожидаем именно `Closed`
        assert!(
            matches!(res, Err(tokio::sync::broadcast::error::RecvError::Closed)),
            "Expected channel to be closed, got {res:?}"
        );
    }

    /// Тест проверяет шаблонную подписку: получает оба сообщения, соответствующие "news:*".
    #[tokio::test]
    async fn test_psubscribe_and_publish() {
        let manager = PubSubManager::new();
        let mut psub = manager.psubscribe("news:*").unwrap();

        manager.publish("news:world", Bytes::from("global"));
        manager.publish("news:local", Bytes::from("city"));

        let msg1 = psub.recv().await.unwrap();
        assert!(msg1.channel.starts_with("news:"));
        let payload1: &[u8] = msg1.payload.as_ref();
        assert!(payload1 == b"global" || payload1 == b"city");

        let msg2 = psub.recv().await.unwrap();
        assert!(msg2.channel.starts_with("news:"));
        let payload2: &[u8] = msg2.payload.as_ref();
        assert!(payload1 != payload2, "Expected two distinct payloads");
    }

    /// Тест проверяет, что некорректный шаблон возвращает ошибку.
    #[test]
    fn test_psubscribe_invalid_pattern() {
        let manager = PubSubManager::new();
        let result = manager.psubscribe("**bad[pattern");
        assert!(result.is_err());
    }

    /// Тест проверяет, что punsubscribe успешно снимает шаблон.
    #[test]
    fn test_punsubscribe_success() {
        let manager = PubSubManager::new();
        let pattern = "log:*";

        manager.psubscribe(pattern).unwrap();
        let result = manager.punsubscribe(pattern);
        assert!(result.is_ok());
    }

    /// Тест проверяет корректность подсчёта статистики publish_count и send_error_count.
    #[test]
    fn test_stats_tracking() {
        let manager = PubSubManager::new();
        manager.publish("any", Bytes::from("1"));
        manager.publish("any", Bytes::from("2"));
        let (published, errors) = manager.stats();

        assert_eq!(published, 2);
        assert_eq!(errors, 0);
    }

    /// Тест проверяет реализацию Default (должна совпадать с new()).
    #[test]
    fn test_default_impl() {
        let defaulted = PubSubManager::default();
        let direct = PubSubManager::new();
        // Примитивное сравнение: просто проверим, что они не паникуют
        assert_eq!(defaulted.stats().0, direct.stats().0);
    }
}
