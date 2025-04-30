use tokio::sync::broadcast;

use crate::Message;

pub trait SubscriptionPort {
    fn receiver(&mut self) -> &mut broadcast::Receiver<Message>;
    fn unsubscribe(self);
}
