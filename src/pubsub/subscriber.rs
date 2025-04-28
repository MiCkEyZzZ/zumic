use tokio::sync::mpsc::Sender;

use super::Message;

/// Subscriber - это просто Sebder<Message>
pub type Subscriber = Sender<Message>;
