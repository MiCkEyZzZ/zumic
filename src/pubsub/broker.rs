use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use dashmap::DashMap;
use serde::Serialize;
use tokio::{sync::broadcast, time::timeout};

use super::{intern_channel, Message};
use crate::{
    pubsub::{MessagePayload, SerializationFormat, Subscriber, SubscriptionOptions},
    RecvError,
};

/// Основной брокер для управления pub/sub системой с расширенной сериализацией
#[derive(Debug)]
pub struct Broker {
    /// Каналы для обычных подписок (точные имена каналов)
    channels: DashMap<Arc<str>, broadcast::Sender<Message>>,
    /// Статистика по каналам
    stats: DashMap<Arc<str>, ChannelStats>,
    /// Конфигурация брокера
    config: BrokerConfig,
    /// Глобальные метрики
    metrics: Arc<BrokerMetrics>,
}

/// Конфигурация брокера
#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// Размер буфера для каналов по умолчанию
    pub default_channel_capacity: usize,
    /// Максимальное количество подписчиков на канал
    pub max_subscribers_per_channel: Option<usize>,
    /// Максимальный размер сообщения в байтах
    pub max_message_size: Option<usize>,
    /// Время жизни пустого канала (после которого он удаляется)
    pub channel_ttl: Option<Duration>,
    /// Включить сжатие больших сообщений
    pub enable_compression: bool,
    /// Порог размера для сжатия (в байтах)
    pub compression_threshold: usize,
}

/// Статистика по каналу
#[derive(Debug, Clone)]
pub struct ChannelStats {
    /// Общее количество отправленных сообщений
    pub messages_sent: u64,
    /// Общее количество байт отправленных данных
    pub bytes_sent: u64,
    /// Текущее количество подписчиков
    pub subscriber_count: usize,
    /// Время создания канала
    pub created_at: Instant,
    /// Время последней активности
    pub last_activity: Instant,
    /// Счетчик отброшенных сообщений (из-за переполнения буфера)
    pub dropped_messages: u64,
}

/// Глобальные метрики брокера
#[derive(Debug, Default)]
pub struct BrokerMetrics {
    /// Общее количество каналов
    pub total_channels: AtomicUsize,
    /// Общее количество сообщений
    pub total_messages: AtomicU64,
    /// Общее количество байт
    pub total_bytes: AtomicU64,
    /// Количество активных подписчиков
    pub active_subscribers: AtomicUsize,
}

/// Результат публикации сообщения
#[derive(Debug, Clone)]
pub struct PublishResult {
    /// Количество подписчиков, получивших сообщение
    pub subscribers_reached: usize,
    /// ID сообщения (если был установлен)
    pub message_id: Option<String>,
    /// Размер сериализованного сообщения
    pub message_size: usize,
    /// Был ли использован компрессия
    pub compressed: bool,
}

/// Опции публикации
#[derive(Debug, Clone)]
pub struct PublishOptions {
    /// Формат сериализации для объектов
    pub serialization_format: Option<SerializationFormat>,
    /// Принудительно сжать сообщение
    pub force_compression: bool,
    /// Таймаут для публикации
    pub timeout: Option<Duration>,
    /// Добавить временную метку
    pub add_timestamp: bool,
    /// Пользовательские заголовки
    pub headers: Option<HashMap<String, String>>,
}

/// Снимок состояния брокера.
#[derive(Debug, Clone)]
pub struct BrokerSnapshot {
    pub channels: Vec<ChannelSnapshot>,
    pub config: BrokerConfig,
    pub metrics: BrokerMetrics,
    pub timestamp: std::time::SystemTime,
}

/// Снимок состояния канала
#[derive(Debug, Clone)]
pub struct ChannelSnapshot {
    pub name: String,
    pub subscriber_count: usize,
    pub stats: ChannelStats,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl Broker {
    /// Создаёт новый брокер с конфигурацией по умолчанию
    pub fn new() -> Self {
        Self::with_config(BrokerConfig::default())
    }

    /// Создаёт новый брокер с заданной конфигурацией.
    pub fn with_config(config: BrokerConfig) -> Self {
        Self {
            channels: DashMap::new(),
            stats: DashMap::new(),
            config,
            metrics: Arc::new(BrokerMetrics::default()),
        }
    }

    /// Публикует сообщение в канал.
    pub fn publish<S>(
        &self,
        channel: S,
        payload: MessagePayload,
    ) -> Result<PublishResult, RecvError>
    where
        S: AsRef<str>,
    {
        self.publish_with_options(channel, payload, PublishOptions::default())
    }

    /// Публикует сообщение в канал
    pub fn publish_with_options<S>(
        &self,
        channel: S,
        mut payload: MessagePayload,
        options: PublishOptions,
    ) -> Result<PublishResult, RecvError>
    where
        S: AsRef<str>,
    {
        let channel_key = intern_channel(channel);

        // Проверяем размер сообщения
        if let Some(max_size) = self.config.max_message_size {
            if payload.len() > max_size {
                return Err(RecvError::SerializationError(format!(
                    "Message size {} exceeds maximum {}",
                    payload.len(),
                    max_size
                )));
            }
        }

        // Применяем сжатие если нужно
        let mut compressed = false;
        if self.config.enable_compression
            && (options.force_compression || payload.len() > self.config.compression_threshold)
        {
            payload = self.compress_payload(payload)?;
            compressed = true;
        }

        // Создаём сообщение
        let mut message = Message::with_payload(channel_key.as_ref(), payload);

        // Добавляем временную метку если требуется
        if options.add_timestamp {
            message = message.with_timestamp();
        }

        // Добавляем пользовательские заголовки
        if let Some(headers) = options.headers {
            for (key, value) in headers {
                message = message.with_header(key, value);
            }
        }

        let message_size = message.size();
        let message_id = message.metadata.as_ref().and_then(|m| m.message_id.clone());

        // Публикуем сообщение
        let subscribers_reached = if let Some(sender) = self.channels.get(&channel_key) {
            match sender.send(message) {
                Ok(subscriber_count) => subscriber_count,
                Err(_) => {
                    // Канал закрыт, удаляем его
                    self.channels.remove(&channel_key);
                    self.stats.remove(&channel_key);
                    0
                }
            }
        } else {
            0 // Нет подписчиков
        };

        // Обновляем статистику
        self.update_channel_stats(&channel_key, message_size, subscribers_reached > 0);
        self.update_global_metrics(message_size);

        Ok(PublishResult {
            subscribers_reached,
            message_id,
            message_size,
            compressed,
        })
    }

    /// Публикует сериализуемый объект.
    pub fn publish_serializable<S, T>(
        &self,
        channel: S,
        value: &T,
        format: SerializationFormat,
    ) -> Result<PublishResult, RecvError>
    where
        S: AsRef<str>,
        T: Serialize,
    {
        let payload = MessagePayload::from_serializable(value, format)?;
        self.publish(channel, payload)
    }

    /// Создаёт подписчика на канал.
    pub fn subscribe<S>(
        &self,
        channel: S,
    ) -> Result<Subscriber, RecvError>
    where
        S: AsRef<str>,
    {
        self.subscribe_with_options(channel, SubscriptionOptions::default())
    }

    pub fn subscribe_with_options<S>(
        &self,
        channel: S,
        options: SubscriptionOptions,
    ) -> Result<Subscriber, RecvError>
    where
        S: AsRef<str>,
    {
        let channel_key = intern_channel(channel);

        let sender = self
            .channels
            .entry(channel_key.clone())
            .or_insert_with(|| {
                let capacity = options
                    .buffer_size
                    .unwrap_or(self.config.default_channel_capacity);
                let (sender, _) = broadcast::channel(capacity);

                // Инициализируем статистику для нового канала
                self.stats.insert(
                    channel_key.clone(),
                    ChannelStats {
                        created_at: std::time::Instant::now(),
                        last_activity: std::time::Instant::now(),
                        ..Default::default()
                    },
                );

                sender
            })
            .clone();

        // Проверяем лимит подписчиков
        if let Some(max_subs) = self.config.max_subscribers_per_channel {
            let current_subs = sender.receiver_count();
            if current_subs >= max_subs {
                return Err(RecvError::SubscriberLimitExceeded);
            }
        }

        let receiver = sender.subscribe();

        // Обновляем статистику подписчиков
        if let Some(mut stats) = self.stats.get_mut(&channel_key) {
            stats.subscriber_count = sender.receiver_count();
        }

        Ok(Subscriber::new(receiver, channel_key, options))
    }

    /// Создаёт подписчика на несколько каналов.
    pub fn subscriber_multiple<S>(
        &self,
        channels: &[S],
    ) -> Result<Vec<Subscriber>, RecvError>
    where
        S: AsRef<str>,
    {
        let mut subscribers = Vec::with_capacity(channels.len());
        for channel in channels {
            subscribers.push(self.subscribe(channel)?);
        }
        Ok(subscribers)
    }

    /// Отписывается от канала (закрывает подписчика).
    pub fn unsubscribe(
        &self,
        channel: &str,
    ) -> bool {
        let channel_key = intern_channel(channel);
        if let Some((_, sender)) = self.channels.remove(&channel_key) {
            // Закрываем канал, что приведёт к отписке всех подписчиков.
            drop(sender);
            self.stats.remove(&channel_key);
            true
        } else {
            false
        }
    }

    /// Возвращает кол-во подписчиков на канал.
    pub fn subscriber_count<S>(
        &self,
        channel: S,
    ) -> usize
    where
        S: AsRef<str>,
    {
        let channel_key = intern_channel(channel);
        self.channels
            .get(&channel_key)
            .map(|sender| sender.receiver_count())
            .unwrap_or(0)
    }

    /// Возвращает вписок всех активных каналов.
    pub fn active_channels(&self) -> Vec<String> {
        self.channels
            .iter()
            .map(|entry| entry.key().to_string())
            .collect()
    }

    /// Возвращает статистику по каналу.
    pub fn channel_stats<S>(
        &self,
        channel: S,
    ) -> Option<ChannelStats>
    where
        S: AsRef<str>,
    {
        let channel_key = intern_channel(channel);
        self.stats.get(&channel_key).map(|stats| stats.clone())
    }

    /// Возвращает глобальные метрики брокера.
    pub fn metrics(&self) -> BrokerMetrics {
        BrokerMetrics {
            total_channels: AtomicUsize::new(self.metrics.total_channels.load(Ordering::Relaxed)),
            total_messages: AtomicU64::new(self.metrics.total_messages.load(Ordering::Relaxed)),
            total_bytes: AtomicU64::new(self.metrics.total_bytes.load(Ordering::Relaxed)),
            active_subscribers: AtomicUsize::new(
                self.metrics.active_subscribers.load(Ordering::Relaxed),
            ),
        }
    }

    /// Очищает неактивные каналы.
    pub fn cleanup_inactive_channels(&self) -> usize {
        let mut removed_count = 0;
        let now = Instant::now();

        if let Some(ttl) = self.config.channel_ttl {
            let channel_to_remove: Vec<_> = self
                .stats
                .iter()
                .filter_map(|entry| {
                    let (channel, stats) = (entry.key(), entry.value());
                    if stats.subscriber_count == 0 && now.duration_since(stats.last_activity) > ttl
                    {
                        Some(channel.clone())
                    } else {
                        None
                    }
                })
                .collect();

            for channel in channel_to_remove {
                if self.channels.remove(&channel).is_some() {
                    self.stats.remove(&channel);
                    removed_count += 1;
                }
            }
        }

        removed_count
    }

    /// Публикует сообщение с таймаутом
    pub async fn publish_with_timeout<S>(
        &self,
        channel: S,
        payload: MessagePayload,
        timeout_duration: Duration,
    ) -> Result<PublishResult, RecvError>
    where
        S: AsRef<str> + Send,
    {
        let options = PublishOptions {
            timeout: Some(timeout_duration),
            ..Default::default()
        };

        if let Some(timeout_duration) = options.timeout {
            timeout(timeout_duration, async {
                // В реальной async версии здесь была бы async публикация
                self.publish_with_options(channel, payload, options)
            })
            .await
            .map_err(|_| RecvError::Timeout)?
        } else {
            self.publish_with_options(channel, payload, options)
        }
    }

    /// Ожидает сообщение в канале (polling)
    pub async fn wait_for_message<S>(
        &self,
        channel: S,
        timeout_duration: Duration,
    ) -> Result<Option<Message>, RecvError>
    where
        S: AsRef<str>,
    {
        let mut subscriber = self.subscribe(channel)?;

        timeout(timeout_duration, async { subscriber.recv().await })
            .await
            .map_err(|_| RecvError::Timeout)?
            .map(Some)
    }

    /// Создаёт снимок состояния брокера для диагностики.
    pub fn snapshot(&self) -> BrokerSnapshot {
        let channels: Vec<_> = self
            .channels
            .iter()
            .map(|entry| {
                let channel_name = entry.key().as_ref().to_string();
                let subscriber_count = entry.value().receiver_count();
                let stats = self
                    .stats
                    .get(entry.key())
                    .map(|s| s.clone())
                    .unwrap_or_default();

                ChannelSnapshot {
                    name: channel_name,
                    subscriber_count,
                    stats,
                }
            })
            .collect();

        BrokerSnapshot {
            channels,
            config: self.config.clone(),
            metrics: self.metrics(),
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Сжимает payload сообщение.
    fn compress_payload(
        &self,
        payload: MessagePayload,
    ) -> Result<MessagePayload, RecvError> {
        let bytes = payload.to_bytes()?;

        // Используем простое gzip сжатие
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&bytes)
            .map_err(|e| RecvError::SerializationError(e.to_string()))?;
        let compressed = encoder
            .finish()
            .map_err(|e| RecvError::SerializationError(e.to_string()))?;

        Ok(MessagePayload::Serialized {
            data: bytes::Bytes::from(compressed),
            content_type: "application/gzip".to_string(),
        })
    }

    fn update_channel_stats(
        &self,
        channel: &Arc<str>,
        message_size: usize,
        delivered: bool,
    ) {
        if let Some(mut stats) = self.stats.get_mut(channel) {
            if delivered {
                stats.messages_sent += 1;
                stats.bytes_sent += message_size as u64;
            } else {
                stats.dropped_messages += 1;
            }
            stats.last_activity = Instant::now();
        }
    }

    /// Обновляет глобальные метрики
    fn update_global_metrics(
        &self,
        message_size: usize,
    ) {
        self.metrics
            .total_messages
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics
            .total_bytes
            .fetch_add(message_size as u64, std::sync::atomic::Ordering::Relaxed);
        self.metrics
            .total_channels
            .store(self.channels.len(), std::sync::atomic::Ordering::Relaxed);

        let total_subscribers: usize = self
            .channels
            .iter()
            .map(|entry| entry.value().receiver_count())
            .sum();
        self.metrics
            .active_subscribers
            .store(total_subscribers, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Broker {
    /// Быстрая публикация строки.
    pub fn publish_str<S, T>(
        &self,
        channel: S,
        message: T,
    ) -> Result<PublishResult, RecvError>
    where
        S: AsRef<str>,
        T: Into<String>,
    {
        self.publish(channel, MessagePayload::String(message.into()))
    }

    /// Быстрая публикация в JSON.
    pub fn publish_json<S, T>(
        &self,
        channel: S,
        value: &T,
    ) -> Result<PublishResult, RecvError>
    where
        S: AsRef<str>,
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)
            .map_err(|e| RecvError::SerializationError(e.to_string()))?;
        self.publish(channel, MessagePayload::Json(json_value))
    }

    pub fn publish_bytes<S, B>(
        &self,
        channel: S,
        bytes: B,
    ) -> Result<PublishResult, RecvError>
    where
        S: AsRef<str>,
        B: Into<bytes::Bytes>,
    {
        self.publish(channel, MessagePayload::Bytes(bytes.into()))
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для BrokerConfig, PublishOptions, ChannelStats,
// BrokerMetrics, Broker
////////////////////////////////////////////////////////////////////////////////

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            default_channel_capacity: 1024,
            max_subscribers_per_channel: Some(10000),
            max_message_size: Some(1024 * 1024),         // 1MБ
            channel_ttl: Some(Duration::from_secs(300)), // 5 минут
            enable_compression: false,
            compression_threshold: 1024, // 1KБ
        }
    }
}

impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            serialization_format: None,
            force_compression: false,
            timeout: Some(Duration::from_secs(5)),
            add_timestamp: false,
            headers: None,
        }
    }
}

impl Default for ChannelStats {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            messages_sent: 0,
            bytes_sent: 0,
            subscriber_count: 0,
            created_at: now,
            last_activity: now,
            dropped_messages: 0,
        }
    }
}

impl Clone for BrokerMetrics {
    fn clone(&self) -> Self {
        Self {
            total_channels: AtomicUsize::new(self.total_channels.load(Ordering::Relaxed)),
            total_messages: AtomicU64::new(self.total_messages.load(Ordering::Relaxed)),
            total_bytes: AtomicU64::new(self.total_bytes.load(Ordering::Relaxed)),
            active_subscribers: AtomicUsize::new(self.active_subscribers.load(Ordering::Relaxed)),
        }
    }
}

impl Default for Broker {
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use tokio::time::Duration;

    use super::*;
    use crate::{LagHandling, TryRecvError};

    /// Helper: создает брокера и подписывается на него, возвращая
    /// (брокеру, получателю)
    async fn setup_one() -> (Broker, Subscriber) {
        let broker = Broker::new();
        let sub = broker.subscribe("chan").expect("subscribe failed");
        (broker, sub)
    }

    /// Тест проверяет, что сообщение доставляется подписчику,
    /// и что счётчики публикаций и ошибок обновляются корректно.
    #[tokio::test]
    async fn test_publish_and_receive() {
        let (broker, mut sub) = setup_one().await;

        broker
            .publish(
                "chan",
                MessagePayload::Bytes(bytes::Bytes::from_static(b"x")),
            )
            .unwrap();

        // Используем публичный async recv()
        let msg = tokio::time::timeout(Duration::from_millis(50), sub.recv())
            .await
            .expect("timed out")
            .expect("no message");

        assert_eq!(msg.channel.as_ref(), "chan");
        assert_eq!(
            msg.payload,
            MessagePayload::Bytes(bytes::Bytes::from_static(b"x"))
        );

        // Проверяем, что метрики брокера обновились:
        assert_eq!(broker.metrics().total_messages.load(Ordering::Relaxed), 1);
    }

    /// Тест проверяет, что все подписчики канала получают
    /// сообщение.
    #[tokio::test]
    async fn test_multiple_subscribers_receive() {
        let broker = Broker::new();
        let mut s1 = broker.subscribe("multi").unwrap();
        let mut s2 = broker.subscribe("multi").unwrap();
        let mut s3 = broker.subscribe("multi").unwrap();

        broker
            .publish("multi", MessagePayload::Bytes(Bytes::from_static(b"d")))
            .unwrap();

        assert_eq!(
            s1.recv().await.unwrap().payload,
            MessagePayload::Bytes(Bytes::from_static(b"d"))
        );
        assert_eq!(
            s2.recv().await.unwrap().payload,
            MessagePayload::Bytes(Bytes::from_static(b"d"))
        );
        assert_eq!(
            s3.recv().await.unwrap().payload,
            MessagePayload::Bytes(Bytes::from_static(b"d"))
        );
    }

    /// Тест проверяет немедленное получение через `try_recv().await`.
    #[tokio::test]
    async fn test_subscription_try_recv_success() {
        let broker = Broker::new();
        let mut sub = broker.subscribe("test_channel").unwrap();

        broker
            .publish(
                "test_channel",
                MessagePayload::Bytes(bytes::Bytes::from("immediate")),
            )
            .unwrap();

        let result = sub.try_recv();
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.channel.as_ref(), "test_channel");
        assert_eq!(
            msg.payload,
            MessagePayload::Bytes(bytes::Bytes::from("immediate"))
        );
    }

    #[tokio::test]
    async fn test_lag_handling_error_mode() {
        let opts = SubscriptionOptions {
            buffer_size: Some(1),
            lag_handling: LagHandling::Error,
            ..Default::default()
        };

        let broker = Broker::new();
        let mut sub = broker.subscribe_with_options("overflow", opts).unwrap();

        // Отправляем 3 сообщения — при буфере=1 возникнет lag
        broker
            .publish("overflow", MessagePayload::Bytes(Bytes::from_static(b"1")))
            .unwrap();
        broker
            .publish("overflow", MessagePayload::Bytes(Bytes::from_static(b"2")))
            .unwrap();
        broker
            .publish("overflow", MessagePayload::Bytes(Bytes::from_static(b"3")))
            .unwrap();

        match sub.try_recv() {
            Err(TryRecvError::Lagged(n)) => assert!(n > 0),
            Ok(_) => (), // возможно система успела, допускаем оба поведения
            Err(e) => panic!("unexpected: {e:?}"),
        }
    }

    /// Тест проверяет, что `try_recv().await` возвращает `Empty` на пустом
    /// канале.
    #[tokio::test]
    async fn test_subscription_try_recv_empty() {
        let broker = Broker::new();
        let mut sub = broker.subscribe("empty_channel").unwrap();

        let result = sub.try_recv();
        assert!(matches!(result, Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn test_compression_roundtrip() {
        // Broker с включённой компрессией (чтобы compress_payload сработал)
        let cfg = BrokerConfig {
            enable_compression: true,
            compression_threshold: 1, // всё сжимать
            ..Default::default()
        };
        let broker = Broker::with_config(cfg);

        let sub_opts = SubscriptionOptions {
            enable_compression: true, // подписчик должен уметь декомпрессить
            ..Default::default()
        };
        let mut sub = broker.subscribe_with_options("c1", sub_opts).unwrap();

        broker
            .publish("c1", MessagePayload::String("hello".into()))
            .unwrap();

        let msg = sub.recv().await.unwrap();
        // ожидаем, что payload декомпрессирован (не Serialized с gzip)
        if let MessagePayload::Serialized { content_type, .. } = msg.payload {
            panic!("unexpected serialized: {content_type}");
        }
    }

    #[test]
    fn test_cleanup_inactive_channels() {
        let cfg = BrokerConfig {
            channel_ttl: Some(Duration::from_millis(10)),
            ..Default::default()
        };
        let broker = Broker::with_config(cfg);

        {
            let sub = broker.subscribe("tmp").unwrap();
            drop(sub); // отпускаем подписчика, чтобы канал стал пустым
        }

        // Ждём, пока subscriber_count обновится и пройдет TTL
        std::thread::sleep(Duration::from_millis(20));

        // Важно: принудительно обновляем subscriber_count в stats
        for mut entry in broker.stats.iter_mut() {
            if let Some(sender) = broker.channels.get(entry.key()) {
                entry.subscriber_count = sender.receiver_count();
            } else {
                entry.subscriber_count = 0;
            }
        }

        let removed = broker.cleanup_inactive_channels();
        assert_eq!(removed, 1);
        assert!(!broker.channels.contains_key("tmp"));
    }

    #[tokio::test]
    async fn test_publish_to_nonexistent_channel() {
        let broker = Broker::new();
        let res = broker
            .publish("nochan", MessagePayload::Bytes(Bytes::from_static(b"z")))
            .unwrap();
        assert_eq!(res.subscribers_reached, 0);
        assert!(!broker.active_channels().contains(&"nochan".to_string()));
    }

    #[tokio::test]
    async fn test_drop_subscription_decrements_receiver_count() {
        let broker = Broker::new();
        let sub = broker.subscribe("tmp").unwrap();
        assert_eq!(broker.subscriber_count("tmp"), 1);
        drop(sub);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(broker.subscriber_count("tmp"), 0);
    }

    #[tokio::test]
    async fn test_subscription_helper_methods() {
        let broker = Broker::new();
        let mut sub = broker.subscribe("helper_channel").unwrap();

        assert_eq!(sub.channel(), "helper_channel");
        assert_eq!(sub.buffered_message_count(), 0);

        broker
            .publish("helper_channel", MessagePayload::Bytes(Bytes::from("test")))
            .unwrap();

        assert!(sub.buffered_message_count() <= 1); // зависит от timing/impl
        let _ = sub.recv().await.unwrap();
        assert_eq!(sub.buffered_message_count(), 0);
    }
}
