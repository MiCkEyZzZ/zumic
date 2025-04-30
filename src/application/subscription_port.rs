//! Интерфейс (порт) для работы с подписками в Pub/Sub системе.
//!
//! Этот трейт описывает операции, которые поддерживаются для подписки:
//! - `receiver` — получить ссылку на приёмник сообщений для подписки.
//! - `unsubscribe` — отписаться от получения сообщений.

use tokio::sync::broadcast;

use crate::Message;

pub trait SubscriptionPort {
    /// Получить ссылку на приёмник сообщений для подписки.
    fn receiver(&mut self) -> &mut broadcast::Receiver<Message>;
    /// Отписаться от получения сообщений.
    fn unsubscribe(self);
}
