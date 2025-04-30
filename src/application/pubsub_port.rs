//! Интерфейс (порт) для управления подписками и публикациями в системе Pub/Sub.
//!
//! Этот трейт описывает операции, связанные с подписками и публикациями:
//! - `psubscribe` — подписаться на канал с использованием шаблона.
//! - `punsubscribe` — отписаться от канала с использованием шаблона.
//! - `subscribe` — подписаться на конкретный канал.
//! - `publish` — опубликовать сообщение на канал.
//! - `unsubscribe_all` — отписаться от всех подписок на канал.

use bytes::Bytes;

use crate::{pubsub::PatternSubscription, Subscription};

pub trait PubSubPort {
    /// Подписаться на канал с использованием шаблона.
    fn psubscribe(&self, pattern: &str) -> Result<PatternSubscription, globset::Error>;
    /// Отписаться от канала с использованием шаблона.
    fn punsubscribe(&self, pattern: &str) -> Result<(), globset::Error>;
    /// Подписаться на конкретный канал.
    fn subscribe(&self, channel: &str) -> Subscription;
    /// Опубликовать сообщение на канал.
    fn publish(&self, channel: &str, payload: Bytes);
    /// Отписаться от всех подписок на канал.
    fn unsubscribe_all(&self, channel: &str);
}
