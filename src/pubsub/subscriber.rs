use tokio::sync::broadcast;

use super::Message;

/// Обёртка над broadcast::Receiver.
/// При Drop (или вызове unsubscribe) просто дропает внутренний Receiver,
/// и клиент автоматически «отписывается».
pub struct Subscription {
    pub channel: String,
    pub inner: broadcast::Receiver<Message>,
}

impl Subscription {
    /// Возвращает mutable-ссылку на сам Receiver, чтобы вызывать .recv().
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Явно отписаться (или просто drop(self))
    pub fn unsubscribe(self) {}
}
