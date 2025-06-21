use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use bytes::Bytes;
use dashmap::DashMap;
use globset::Glob;
use tokio::sync::broadcast;

use super::{intern_channel, Message, PatternSubscription, Subscription};

type ChannelKey = Arc<str>;
type PatternKey = Glob;

/// Pub/Sub message broker.
///
/// Features:
/// - Exact subscriptions by channel name
/// - Pattern subscriptions (glob-based)
/// - Automatic removal of empty channels
/// - Statistics tracking for publishes and send errors
#[derive(Debug)]
pub struct Broker {
    /// Exact channel → `Sender`
    channels: Arc<DashMap<ChannelKey, broadcast::Sender<Message>>>,
    /// Pattern (glob) → `Sender`
    patterns: Arc<DashMap<PatternKey, broadcast::Sender<Message>>>,
    /// Buffer size for each `broadcast::channel`
    default_capacity: usize,
    /// Total number of `publish` calls
    pub publish_count: AtomicUsize,
    /// Number of failed `send` operations (no subscribers)
    pub send_error_count: AtomicUsize,
}

impl Broker {
    /// Creates a new `Broker` with the given buffer capacity.
    pub fn new(default_capacity: usize) -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
            patterns: Arc::new(DashMap::new()),
            default_capacity,
            publish_count: AtomicUsize::new(0),
            send_error_count: AtomicUsize::new(0),
        }
    }
}

impl Broker {
    /// Subscribes to a glob pattern, e.g. `"kin.*"` or `"a?c"`.
    ///
    /// Re-subscribing to the same pattern returns the same `Sender`.
    pub fn psubscribe(&self, pattern: &str) -> Result<PatternSubscription, globset::Error> {
        let glob = Glob::new(pattern)?;
        let tx = self
            .patterns
            .entry(glob.clone())
            .or_insert_with(|| broadcast::channel(self.default_capacity).0)
            .clone();
        Ok(PatternSubscription {
            pattern: glob,
            inner: tx.subscribe(),
        })
    }

    /// Unsubscribes from a pattern. Removes the associated `Sender`.
    pub fn punsubscribe(&self, pattern: &str) -> Result<(), globset::Error> {
        let glob = Glob::new(pattern)?;
        self.patterns.remove(&glob);
        Ok(())
    }

    /// Subscribes to an exact channel name.
    ///
    /// An `Arc<str>` key is interned on first subscription.
    pub fn subscribe(&self, channel: &str) -> Subscription {
        let key: Arc<str> = intern_channel(channel);
        let tx = self
            .channels
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(self.default_capacity).0)
            .clone();
        Subscription {
            channel: key,
            inner: tx.subscribe(),
        }
    }

    /// Publishes a message to a channel.
    ///
    /// Works in two steps:
    /// 1. Sends to the exact channel (if present)
    /// 2. Sends to all matching pattern subscribers
    ///
    /// If the exact channel has no subscribers, `send_error_count` is incremented
    /// and the channel is removed.
    pub fn publish(&self, channel: &str, payload: Bytes) {
        self.publish_count.fetch_add(1, Ordering::Relaxed);

        // 1) exact match
        if let Some(entry) = self.channels.get_mut(channel) {
            let tx = entry.value().clone();
            let msg = Message::new(entry.key().clone(), payload.clone());
            if tx.send(msg).is_err() {
                self.send_error_count.fetch_add(1, Ordering::Relaxed);
            }
            if tx.receiver_count() == 0 {
                let key = entry.key().clone();
                drop(entry);
                self.channels.remove(&*key);
            }
        }

        // 2) pattern match
        for entry in self.patterns.iter() {
            let matcher = entry.key().compile_matcher();
            if matcher.is_match(channel) {
                let tx = entry.value().clone();
                let msg = Message::new(channel, payload.clone());
                let _ = tx.send(msg);
            }
        }
    }

    /// Removes all subscriptions to a given channel and deletes the channel.
    ///
    /// Future `publish` calls will not re-create the channel.
    pub fn unsubscribe_all(&self, channel: &str) {
        self.channels.remove(channel);
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use tokio::sync::broadcast::error::RecvError;
    use tokio::time::{timeout, Duration};

    use super::*;

    /// Helper: creates a broker and subscribes to it, returning (broker, receiver)
    async fn setup_one() -> (Broker, tokio::sync::broadcast::Receiver<Message>) {
        let broker = Broker::new(5);
        let Subscription { inner: rx, .. } = broker.subscribe("chan");
        (broker, rx)
    }

    /// Checks that a message is delivered to a subscriber,
    /// and that publish counters are updated correctly.
    #[tokio::test]
    async fn test_publish_and_receive() {
        let (broker, mut rx) = setup_one().await;
        broker.publish("chan", Bytes::from_static(b"x"));
        let msg = timeout(Duration::from_millis(50), rx.recv())
            .await
            .expect("timed out")
            .expect("no message");
        assert_eq!(&*msg.channel, "chan");
        assert_eq!(msg.payload, Bytes::from_static(b"x"));
        // publish_count should be 1, send_error_count == 0
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
    }

    /// Checks that publishing to a non-existent channel
    /// does not create the channel or increment send_error_count.
    #[tokio::test]
    async fn test_publish_to_nonexistent_channel() {
        let broker = Broker::new(5);
        broker.publish("nochan", Bytes::from_static(b"z"));
        // No subscribers, channel is not created, send_error is not incremented
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("nochan"));
    }

    /// Checks that all subscribers to a channel receive the message.
    #[tokio::test]
    async fn test_multiple_subscribers_receive() {
        let broker = Broker::new(5);
        let subs = (0..3)
            .map(|_| broker.subscribe("multi"))
            .map(|s| s.inner)
            .collect::<Vec<_>>();

        broker.publish("multi", Bytes::from_static(b"d"));
        for mut rx in subs {
            let msg = timeout(Duration::from_millis(50), rx.recv())
                .await
                .expect("timed out")
                .expect("no msg");
            assert_eq!(&*msg.channel, "multi");
            assert_eq!(msg.payload, Bytes::from_static(b"d"));
        }
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
    }

    /// Checks that if the subscription is dropped and no one listens,
    /// publishing triggers send_error and the channel is removed.
    #[tokio::test]
    async fn test_auto_remove_empty_channel_and_error_count() {
        // 1) subscribe and immediately drop the subscription
        let broker = Broker::new(5);
        {
            let sub = broker.subscribe("temp");
            drop(sub);
        }
        // channel still exists until first publish
        assert!(broker.channels.contains_key("temp"));

        // 2) publishing should trigger send_error and remove the channel
        broker.publish("temp", Bytes::from_static(b"u"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 1);
        assert!(!broker.channels.contains_key("temp"));
    }

    /// Checks that after `unsubscribe_all`, future publications are ignored.
    #[tokio::test]
    async fn test_unsubscribe_all() {
        let broker = Broker::new(5);
        let _sub = broker.subscribe("gone");
        // now remove all subscriptions
        broker.unsubscribe_all("gone");
        assert!(!broker.channels.contains_key("gone"));

        // publishing after removal increments publish_count,
        // but not send_error_count, and channel is not recreated
        broker.publish("gone", Bytes::from_static(b"x"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("gone"));
    }

    /// Checks that pattern-based (psubscribe) subscriptions receive messages.
    #[tokio::test]
    async fn test_psubscribe_and_receive() {
        let broker = Broker::new(5);
        let mut psub = broker.psubscribe("foo.*").unwrap();
        // exact channel also matches the pattern
        broker.publish("foo.bar", Bytes::from_static(b"X"));
        let msg = psub.receiver().recv().await.expect("no msg");
        assert_eq!(&*msg.channel, "foo.bar");
        assert_eq!(msg.payload, Bytes::from_static(b"X"));
    }

    /// Checks that normal and pattern subscriptions work together.
    #[tokio::test]
    async fn test_sub_and_psub_together() {
        let broker = Broker::new(5);
        let mut sub = broker.subscribe("topic");
        let mut psub = broker.psubscribe("t*").unwrap();

        broker.publish("topic", Bytes::from_static(b"Z"));

        let m1 = sub.receiver().recv().await.expect("no exact");
        let m2 = psub.receiver().recv().await.expect("no pattern");
        assert_eq!(&*m1.channel, "topic");
        assert_eq!(&*m2.channel, "topic");
        assert_eq!(m1.payload, Bytes::from_static(b"Z"));
        assert_eq!(m2.payload, Bytes::from_static(b"Z"));
    }

    /// Checks that after `punsubscribe`, the receiver is closed.
    #[tokio::test]
    async fn test_punsubscribe_no_receive() {
        let broker = Broker::new(5);
        let mut psub = broker.psubscribe("a?c").unwrap();
        // remove pattern from broker
        broker.punsubscribe("a?c").unwrap();
        // no remaining Senders, Receiver should get Closed
        let res = psub.receiver().recv().await;
        use tokio::sync::broadcast::error::RecvError;
        assert!(matches!(res, Err(RecvError::Closed)));
    }

    /// Checks that two different subscribers to the same channel
    /// both receive each published message.
    #[tokio::test]
    async fn test_multiple_subscribe_same_channel() {
        let broker = Broker::new(5);

        let mut sub1 = broker.subscribe("dup");
        let mut sub2 = broker.subscribe("dup");
        let rx1 = sub1.receiver();
        let rx2 = sub2.receiver();

        broker.publish("dup", Bytes::from_static(b"hi"));

        let msg1 = rx1.recv().await.unwrap();
        let msg2 = rx2.recv().await.unwrap();

        assert_eq!(&*msg1.channel, "dup");
        assert_eq!(&*msg2.channel, "dup");
        assert_eq!(msg1.payload, Bytes::from_static(b"hi"));
        assert_eq!(msg2.payload, Bytes::from_static(b"hi"));
    }

    /// Checks that dropping a Subscription
    /// reduces the broadcast sender's receiver count to zero.
    #[tokio::test]
    async fn test_drop_subscription_decrements_receiver_count() {
        let broker = Broker::new(5);
        let sub = broker.subscribe("tmp");
        let key = Arc::clone(&sub.channel);
        let sender = broker.channels.get(&*key).unwrap().clone();
        assert_eq!(sender.receiver_count(), 1);
        drop(sub);
        // Give time for drop to propagate
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(sender.receiver_count(), 0);
    }

    /// Checks broadcast behavior on buffer overflow:
    /// old message is dropped, and recv() returns `Lagged(1)`.
    #[tokio::test]
    async fn test_broadcast_overwrites_when_buffer_full() {
        let broker = Broker::new(1); // buffer size = 1

        // Hold onto Subscription so it's not dropped
        let mut subscription = broker.subscribe("overflow");
        let sub = subscription.receiver();

        // Send the first message
        broker.publish("overflow", Bytes::from_static(b"first"));
        // Send the second message — it should evict the first
        broker.publish("overflow", Bytes::from_static(b"second"));

        // Receiving should yield Err(Lagged(1)) due to message loss
        let err = sub.recv().await.unwrap_err();
        assert!(
            matches!(err, RecvError::Lagged(1)),
            "Expected Lagged(1), got: {err:?}"
        );
    }

    /// Checks that psubscribe returns an error for invalid glob patterns.
    #[tokio::test]
    async fn test_psubscribe_invalid_pattern() {
        let broker = Broker::new(5);
        let res = broker.psubscribe("[invalid");
        assert!(res.is_err());
    }

    /// Checks that after unsubscribe_all, the channel is not recreated
    /// and statistics are updated correctly.
    #[tokio::test]
    async fn test_publish_after_unsubscribe_all_does_not_create_channel() {
        let broker = Broker::new(5);
        let _ = broker.subscribe("vanish");
        broker.unsubscribe_all("vanish");
        assert!(!broker.channels.contains_key("vanish"));

        broker.publish("vanish", Bytes::from_static(b"y"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("vanish")); // channel shouldn't be re-created
    }
}
