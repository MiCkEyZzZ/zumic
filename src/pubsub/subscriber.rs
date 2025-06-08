use std::sync::Arc;

use globset::Glob;
use tokio::sync::broadcast;

use super::Message;

/// Subscription to a specific pub/sub channel.
///
/// Wraps a [`broadcast::Receiver`] associated with a channel name (`Arc<str>`),
/// allowing you to receive messages from that channel.
///
/// Unsubscription happens automatically on `Drop`, or can be done explicitly via [`Subscription::unsubscribe`].
pub struct Subscription {
    /// The channel name subscribed to.
    pub channel: Arc<str>,
    /// The internal `broadcast::Receiver` for incoming messages.
    pub inner: broadcast::Receiver<Message>,
}

/// Pattern-based subscription to multiple channels.
///
/// Uses [`globset::Glob`] to match channel names, and receives messages
/// from all matching channels.
///
/// Unsubscription also happens automatically on `Drop`, or can be done explicitly via [`PatternSubscription::unsubscribe`].
pub struct PatternSubscription {
    /// The glob pattern used to match channel names.
    pub pattern: Glob,
    /// The internal `broadcast::Receiver` for incoming messages.
    pub inner: broadcast::Receiver<Message>,
}

impl Subscription {
    /// Returns a mutable reference to the internal [`broadcast::Receiver`],
    /// so you can call `.recv().await`.
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Explicitly unsubscribe from the channel. Equivalent to `drop(self)`.
    ///
    /// After calling this, no more messages will be received.
    pub fn unsubscribe(self) {
        // Nothing to do: dropping the inner Receiver removes it from the broadcast channel
    }
}

impl PatternSubscription {
    /// Returns a mutable reference to the internal [`broadcast::Receiver`],
    /// so you can call `.recv().await`.
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<Message> {
        &mut self.inner
    }

    /// Explicitly unsubscribe from the pattern subscription.
    ///
    /// After calling this, no more messages matching the pattern will be received.
    pub fn unsubscribe(self) {
        // Dropping the Receiver removes it from the broadcast channel
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use globset::Glob;
    use tokio::{sync::broadcast, time::timeout};

    use crate::{pubsub::PatternSubscription, Broker, Subscription};

    /// Verifies that a subscription retains the correct channel name.
    #[tokio::test]
    async fn test_subscription_channel_name() {
        let sub = {
            let broker = Broker::new(10);
            let sub = broker.subscribe("mychan");
            assert_eq!(&*sub.channel, "mychan");
            sub
        };
        // After broker goes out of scope, the subscriber still holds the channel
        assert_eq!(&*sub.channel, "mychan");
    }

    /// Verifies that a published message is received by the subscription.
    #[tokio::test]
    async fn test_receive_message_via_subscription() {
        let broker = Broker::new(10);
        let mut sub = broker.subscribe("testchan");
        broker.publish("testchan", Bytes::from_static(b"hello"));
        let msg = timeout(Duration::from_millis(100), sub.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg.channel, "testchan");
        assert_eq!(msg.payload, Bytes::from_static(b"hello"));
    }

    /// Verifies that dropping the subscription decreases the receiver count.
    #[test]
    fn test_unsubscribe_drops_receiver() {
        let (tx, rx) = broadcast::channel(5);
        let channel_arc: Arc<str> = Arc::from("foo");
        let sub = Subscription {
            channel: channel_arc.clone(),
            inner: rx,
        };
        assert_eq!(tx.receiver_count(), 1);
        drop(sub);
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Verifies that calling `unsubscribe` explicitly consumes the subscription.
    #[test]
    fn test_explicit_unsubscribe_consumes_subscription() {
        let (tx, rx) = broadcast::channel(5);
        let channel_arc: Arc<str> = Arc::from("bar");
        let sub = Subscription {
            channel: channel_arc,
            inner: rx,
        };
        assert_eq!(tx.receiver_count(), 1);
        sub.unsubscribe();
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Verifies that pattern subscription (`psubscribe`) correctly receives messages.
    #[tokio::test]
    async fn test_pattern_subscription_receives_message() {
        let broker = Broker::new(10);
        let mut psub = broker.psubscribe("foo*").unwrap();

        broker.publish("foobar", Bytes::from_static(b"xyz"));

        let msg = timeout(Duration::from_millis(100), psub.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");

        assert_eq!(&*msg.channel, "foobar");
        assert_eq!(msg.payload, Bytes::from_static(b"xyz"));
    }

    /// Verifies that after unsubscribing the pattern, no messages are received.
    #[tokio::test]
    async fn test_pattern_unsubscribe_stops_reception() {
        let broker = Broker::new(10);
        let mut psub = broker.psubscribe("bar*").unwrap();
        broker.punsubscribe("bar*").unwrap();
        broker.publish("barbaz", Bytes::from_static(b"nope"));
        let result = timeout(Duration::from_millis(100), psub.receiver().recv()).await;
        assert!(
            result.is_err() || matches!(result.unwrap(), Err(broadcast::error::RecvError::Closed))
        );
    }

    /// Verifies that multiple pattern subscriptions receive the same message.
    #[tokio::test]
    async fn test_multiple_pattern_subscriptions_receive() {
        let broker = Broker::new(10);
        let mut ps1 = broker.psubscribe("qu?x").unwrap();
        let mut ps2 = broker.psubscribe("qu*").unwrap();

        broker.publish("quux", Bytes::from_static(b"hello"));

        let msg1 = timeout(Duration::from_millis(100), ps1.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg1.channel, "quux");
        assert_eq!(msg1.payload, Bytes::from_static(b"hello"));

        let msg2 = timeout(Duration::from_millis(100), ps2.receiver().recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg2.channel, "quux");
        assert_eq!(msg2.payload, Bytes::from_static(b"hello"));
    }

    /// Verifies that dropping a pattern subscription decreases the receiver count.
    #[test]
    fn test_pattern_unsubscribe_drops_receiver() {
        let (tx, rx) = broadcast::channel(3);
        let pattern = Glob::new("pat*").unwrap();

        let psub = PatternSubscription { pattern, inner: rx };
        assert_eq!(tx.receiver_count(), 1);
        drop(psub);
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Verifies that calling `unsubscribe` on a pattern subscription consumes it.
    #[test]
    fn test_pattern_explicit_unsubscribe_consumes() {
        let (tx, rx) = broadcast::channel(3);
        let pattern = Glob::new("z*").unwrap();

        let psub = PatternSubscription { pattern, inner: rx };
        assert_eq!(tx.receiver_count(), 1);
        psub.unsubscribe();
        assert_eq!(tx.receiver_count(), 0);
    }

    /// Verifies that two subscriptions to the same channel both receive each message.
    #[tokio::test]
    async fn test_double_subscribe_same_channel() {
        let broker = Broker::new(5);
        let mut a = broker.subscribe("dup");
        let mut b = broker.subscribe("dup");
        broker.publish("dup", Bytes::from_static(b"X"));
        assert_eq!(
            a.receiver().recv().await.unwrap().payload,
            Bytes::from_static(b"X")
        );
        assert_eq!(
            b.receiver().recv().await.unwrap().payload,
            Bytes::from_static(b"X")
        );
    }

    /// Verifies that unsubscribing from a non-existent channel or pattern
    /// does not panic and returns cleanly.
    #[test]
    fn test_unsubscribe_nonexistent() {
        let broker = Broker::new(5);
        // оба должны просто вернуться без паники.
        broker.unsubscribe_all("nochan");
        broker.punsubscribe("no*pat").unwrap();
    }

    /// Verifies that subscribing with an invalid glob pattern returns a parse error.
    #[test]
    fn test_invalid_glob_pattern() {
        let broker = Broker::new(5);
        assert!(broker.psubscribe("[invalid[").is_err());
    }
}
