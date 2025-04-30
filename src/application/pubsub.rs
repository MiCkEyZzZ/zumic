use bytes::Bytes;

use crate::{pubsub::PatternSubscription, Subscription};

pub trait PubSubPort {
    fn psubscribe(&self, pattern: &str) -> Result<PatternSubscription, globset::Error>;
    fn punsubscribe(&self, pattern: &str) -> Result<(), globset::Error>;
    fn subscribe(&self, channel: &str) -> Subscription;
    fn publish(&self, channel: &str, payload: Bytes);
    fn unsubscribe_all(&self, channel: &str);
}
