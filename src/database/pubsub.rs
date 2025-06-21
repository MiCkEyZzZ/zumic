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
    pub fn publish(&self, channel: &str, payload: Bytes) {
        self.broker.publish(channel, payload);
    }

    /// Подписка на точный канал.
    pub fn subscribe(&self, channel: &str) -> Subscription {
        self.broker.subscribe(channel)
    }

    /// Отписаться от всех подписок на канал.
    pub fn unsubscribe_all(&self, channel: &str) {
        self.broker.unsubscribe_all(channel);
    }

    /// Подписка по шаблону.
    pub fn psubscribe(&self, pattern: &str) -> Result<PatternSubscription, GlobError> {
        self.broker.psubscribe(pattern)
    }

    /// Отписаться от шаблона.
    pub fn punsubscribe(&self, pattern: &str) -> Result<(), GlobError> {
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
