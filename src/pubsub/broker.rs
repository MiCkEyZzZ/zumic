use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use bytes::Bytes;
use dashmap::DashMap;
use globset::Glob;
use tokio::sync::broadcast;

use super::{Message, PatternSubscription, Subscription};

type ChannelKey = Arc<str>;
type PatternKey = Glob;

/// Брокер Pub/Sub сообщений.
///
/// Поддерживает:
/// - Точные подписки по имени канала
/// - Подписки по шаблонам (glob)
/// - Автоматическое удаление пустых каналов
/// - Статистику публикаций и ошибок отправки
pub struct Broker {
    /// Точные каналы → `Sender`
    channels: Arc<DashMap<ChannelKey, broadcast::Sender<Message>>>,
    /// Шаблоны каналов → `Sender`
    patterns: Arc<DashMap<PatternKey, broadcast::Sender<Message>>>,
    /// Ёмкость буфера каждого `broadcast::channel`
    default_capacity: usize,
    /// Общее количество вызовов `publish`
    pub publish_count: AtomicUsize,
    /// Количество неудачных `send` (нет подписчиков)
    pub send_error_count: AtomicUsize,
}

impl Broker {
    /// Создаёт новый `Broker` с заданной буферной ёмкостью.
    pub fn new(default_capacity: usize) -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
            patterns: Arc::new(DashMap::new()),
            default_capacity,
            publish_count: AtomicUsize::new(0),
            send_error_count: AtomicUsize::new(0),
        }
    }

    /// Подписка по шаблону (glob), например `"kin.*"` или `"a?c"`.
    ///
    /// Повторная подписка на тот же шаблон получит тот же `Sender`.
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

    /// Отписка от шаблона. Удаляет соответствующий `Sender`.
    pub fn punsubscribe(&self, pattern: &str) -> Result<(), globset::Error> {
        let glob = Glob::new(pattern)?;
        self.patterns.remove(&glob);
        Ok(())
    }

    /// Подписка на конкретный канал (точное совпадение).
    ///
    /// Создаёт `Arc<str>` ключ при первой подписке.
    pub fn subscribe(&self, channel: &str) -> Subscription {
        let key: Arc<str> = Arc::from(channel);
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

    /// Публикация сообщения в канал.
    ///
    /// Работает в два этапа:
    /// 1. Отправляет в точный канал (если есть)
    /// 2. Отправляет всем подписчикам по шаблонам
    ///
    /// Если в точном канале нет подписчиков — увеличивает `send_error_count`
    /// и удаляет канал.
    pub fn publish(&self, channel: &str, payload: Bytes) {
        self.publish_count.fetch_add(1, Ordering::Relaxed);

        // 1) точное совпадение
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

        // 2) по шаблону
        for entry in self.patterns.iter() {
            let matcher = entry.key().compile_matcher();
            if matcher.is_match(channel) {
                let tx = entry.value().clone();
                let msg = Message::new(channel.to_string(), payload.clone());
                let _ = tx.send(msg);
            }
        }
    }

    /// Удаляет все подписки на указанный канал (и сам канал).
    ///
    /// Следующая `publish` не создаст канал заново.
    pub fn unsubscribe_all(&self, channel: &str) {
        self.channels.remove(channel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio::time::{timeout, Duration};

    /// Helper: создаёт брокер и сразу подписывается, возвращая (broker, receiver)
    async fn setup_one() -> (Broker, tokio::sync::broadcast::Receiver<Message>) {
        let broker = Broker::new(5);
        let Subscription { inner: rx, .. } = broker.subscribe("chan");
        (broker, rx)
    }

    /// Проверяет, что сообщение успешно доставляется подписчику,
    /// и что счётчики публикации обновлены правильно.
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
        // publish_count должно быть 1, send_error_count == 0
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
    }

    /// Проверяет, что публикация в несуществующий канал
    /// не создаёт его и не инкрементирует send_error_count.
    #[tokio::test]
    async fn test_publish_to_nonexistent_channel() {
        let broker = Broker::new(5);
        broker.publish("nochan", Bytes::from_static(b"z"));
        // Нет подписчиков, канал не создаётся, send_error не инкрементится
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("nochan"));
    }

    /// Проверяет, что все подписчики на канал получают сообщение.
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

    /// Проверяет, что если после drop'а подписки никто не слушает канал,
    /// публикация вызывает send_error и канал удаляется.
    #[tokio::test]
    async fn test_auto_remove_empty_channel_and_error_count() {
        // 1) подписываемся и сразу дропаем подписку
        let broker = Broker::new(5);
        {
            let sub = broker.subscribe("temp");
            drop(sub);
        }
        // канал всё ещё есть до первой публикации
        assert!(broker.channels.contains_key("temp"));

        // 2) публикация должна дать send_error и удалить канал
        broker.publish("temp", Bytes::from_static(b"u"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 1);
        assert!(!broker.channels.contains_key("temp"));
    }

    /// Проверяет, что после вызова `unsubscribe_all`, публикации игнорируются.
    #[tokio::test]
    async fn test_unsubscribe_all() {
        let broker = Broker::new(5);
        let _sub = broker.subscribe("gone");
        // теперь удаляем все подписки
        broker.unsubscribe_all("gone");
        assert!(!broker.channels.contains_key("gone"));

        // публикация после полного удаления — publish_count инкрементируется,
        // но send_error_count не, и канал не создаётся заново
        broker.publish("gone", Bytes::from_static(b"x"));
        assert_eq!(broker.publish_count.load(Ordering::Relaxed), 1);
        assert_eq!(broker.send_error_count.load(Ordering::Relaxed), 0);
        assert!(!broker.channels.contains_key("gone"));
    }

    /// Проверяет, что подписка по шаблону (psubscribe) получает сообщение.
    #[tokio::test]
    async fn test_psubscribe_and_receive() {
        let broker = Broker::new(5);
        let mut psub = broker.psubscribe("foo.*").unwrap();
        // точный канал тоже не влияет
        broker.publish("foo.bar", Bytes::from_static(b"X"));
        let msg = psub.receiver().recv().await.expect("no msg");
        assert_eq!(&*msg.channel, "foo.bar");
        assert_eq!(msg.payload, Bytes::from_static(b"X"));
    }

    /// Проверяет, что одновременно работают обычная и шаблонная подписки.
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

    /// Проверяет, что после `punsubscribe` Receiver закрывается.
    #[tokio::test]
    async fn test_punsubscribe_no_receive() {
        let broker = Broker::new(5);
        let mut psub = broker.psubscribe("a?c").unwrap();
        // убираем шаблон из брокера
        broker.punsubscribe("a?c").unwrap();
        // после удаления в брокере не остаётся ни одного Sender,
        // поэтому этот Receiver должен получить Closed
        let res = psub.receiver().recv().await;
        use tokio::sync::broadcast::error::RecvError;
        assert!(matches!(res, Err(RecvError::Closed)));
    }
}
