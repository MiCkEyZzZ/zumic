use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use tokio::sync::broadcast;

use super::{Message, Subscription};

const DEFAULT_CAPACITY: usize = 100;

/// Брокер Pub/Sub сообщений.
pub struct Broker {
    // для каждого канала — свой broadcast::Sender
    channels: Arc<DashMap<String, broadcast::Sender<Message>>>,
}

impl Broker {
    /// Создатёт новый брокер.
    pub fn new() -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
        }
    }

    /// Подписаться на канал.
    pub fn subscribe(&self, channel: &str) -> Subscription {
        // получаем или создаём Sender для этого канала
        let sender = self
            .channels
            .entry(channel.to_string())
            .or_insert_with(|| broadcast::channel(DEFAULT_CAPACITY).0)
            .clone();

        // создаём новый Receiver и кладём в Subscription
        let rx = sender.subscribe();
        Subscription {
            channel: channel.to_string(),
            inner: rx,
        }
    }

    /// Публиковать сообщения в канал.
    pub fn publish(&self, channel: &str, payload: Bytes) {
        if let Some(tx) = self.channels.get(channel) {
            // игнорируем error, если нет ни одного подписчика.
            let _ = tx.send(Message::new(channel.to_string(), payload));
        }
    }

    /// Удалить все подписки на канал (закрыть канал).
    pub fn unsubscribe_all(&self, channel: &str) {
        // удаляем Sender — все Receiver при этом получат RecvError::Closed
        self.channels.remove(channel);
    }
}
