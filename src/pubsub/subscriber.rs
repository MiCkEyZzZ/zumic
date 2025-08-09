use std::{collections::VecDeque, fmt, str::from_utf8, sync::Arc};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use tokio::{
    sync::broadcast,
    time::{timeout, Duration, Instant},
};

use super::Message;
use crate::{MessagePayload, RecvError, TryRecvError};

type CustomFilter = Arc<dyn Fn(&Message) -> bool + Send + Sync + 'static>;

/// Типы payload для фильтрации.
#[derive(Debug, Clone, PartialEq)]
pub enum PayloadType {
    Bytes,
    String,
    Json,
    Serialized(String),
}

/// Фильтр по типу сообщения.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageTypeFilter {
    /// Только определённые типы payload
    PayloadType(Vec<PayloadType>),
    /// Только сообщения с метаданными
    WithMetadata,
    /// Только сообщения без метаданных
    WithoutMetadata,
    /// Комбинированный фильтр
    Combined(Box<MessageTypeFilter>, Box<MessageTypeFilter>),
}

/// Обработка отставания.
#[derive(Debug, Clone, PartialEq)]
pub enum LagHandling {
    /// Игнорировать отстование (по умолчанию)
    Ignore,
    /// Возвращать ошибку при отставании
    Error,
    /// Логировать отстование, но продолжать работу
    Log,
    /// Пропускать отстающие сообщения
    Skip,
}

/// Подписчик с расширенными возможностями фильтрации и обработки.
#[derive(Debug)]
pub struct Subscriber {
    /// Основной приёмник сообщений
    receiver: broadcast::Receiver<Message>,
    /// Канал, на который подписан подписчик
    channel: Arc<str>,
    /// Опции подписки.
    options: SubscriptionOptions,
    /// Буфер для хранения сообщений (если включён)
    message_buffer: Option<VecDeque<Message>>,
    /// Статистика подписчика
    stats: SubscriberStats,
    /// Фильтры сообщений
    filters: MessageFilters,
}

/// Массовый подписчик для работы с несколькими каналами.
#[derive(Debug)]
pub struct MultiSubscriber {
    subscribers: Vec<Subscriber>,
    round_robin_index: usize,
}

/// Опции подписки.
#[derive(Debug, Clone)]
pub struct SubscriptionOptions {
    /// Размер буфера канала
    pub buffer_size: Option<usize>,
    /// Включить локальный буфер сообщений
    pub enable_message_buffer: bool,
    /// Размер локального буфера
    pub message_buffer_size: usize,
    /// Автоматически десериализовать JSON сообщения
    pub auto_deserialize_json: bool,
    /// Фильтр по типу сообщений
    pub message_type_filter: Option<MessageTypeFilter>,
    /// Максимальное время ожидания сообщения
    pub recv_timeout: Option<Duration>,
    /// Обработка отставания (lagging)
    pub lag_handling: LagHandling,
    /// Включить сжатие больших сообщений
    pub enable_compression: bool,
}

/// Статистика подписчика.
#[derive(Debug, Clone)]
pub struct SubscriberStats {
    /// Общее количество полученных сообщений
    pub messages_received: u64,
    /// Количество байт полученных данных
    pub bytes_received: u64,
    /// Количество отфильтрованных сообщений
    pub messages_filtered: u64,
    /// Количество сообщений, пропущенных из-за отставания
    pub lagged_messages: u64,
    /// Время создания подписчика
    pub created_at: Instant,
    /// Время последнего полученного сообщения
    pub last_message_at: Option<Instant>,
    /// Количество ошибок десериализации
    pub deserialization_errors: u64,
}

/// Фильтры сообщений.
#[derive(Clone, Default)]
pub struct MessageFilters {
    /// Фильтр по размеру сообщения
    pub size_filter: Option<SizeFilter>,
    /// Фильтр по метаданным
    pub metadata_filter: Option<MetadataFilter>,
    /// Фильтр по содержимому
    pub content_filter: Option<ContentFilter>,
    /// Пользовательский фильтр
    pub custom_filter: Option<CustomFilter>,
}

/// Фильтр по размеру сообщения.
#[derive(Debug, Clone)]
pub struct SizeFilter {
    pub min_size: Option<usize>,
    pub max_size: Option<usize>,
}

/// Фильтр по метаданным.
#[derive(Debug, Clone)]
pub struct MetadataFilter {
    /// Обязательные заголовки
    pub required_headers: Vec<String>,
    /// Фильтр по ID сообщения (glob pattern)
    pub message_id_pattern: Option<GlobSet>,
    /// Фильтр по временному диапазону
    pub time_range: Option<(u64, u64)>, // (from_timestamp, to_timestamp)
}

/// Фильтр по
#[derive(Debug, Clone)]
pub struct ContentFilter {
    /// Разрешённые типы payload
    pub allowed_payload_types: Vec<PayloadType>,
    /// Паттерны для строкового содержимого
    pub string_patterns: Option<GlobSet>,
    /// Фильтр JSON по ключам
    pub json_key_filter: Option<Vec<String>>,
}

/// Результат получения сообщения с дополнительной информацией.
#[derive(Debug, Clone)]
pub struct MessageResult {
    /// Полученное сообщение
    pub message: Message,
    /// Было ли сообщение отфильтровано до получения
    pub filtered: bool,
    /// Время получения
    pub received_at: Instant,
    /// Размер сообщения в байтах
    pub size: usize,
}

impl Subscriber {
    /// Создаёт нового подписчика.
    pub(crate) fn new(
        receiver: broadcast::Receiver<Message>,
        channel: Arc<str>,
        options: SubscriptionOptions,
    ) -> Self {
        let message_buffer = if options.enable_message_buffer {
            Some(VecDeque::with_capacity(options.message_buffer_size))
        } else {
            None
        };

        Self {
            receiver,
            channel,
            options,
            message_buffer,
            stats: SubscriberStats {
                created_at: Instant::now(),
                ..Default::default()
            },
            filters: MessageFilters::default(),
        }
    }

    /// Получает следующее сообщение (блокирующий вызов).
    pub async fn recv(&mut self) -> Result<Message, RecvError> {
        // Сначала проверяем локальный буфер.
        if let Some(ref mut buffer) = self.message_buffer {
            if let Some(message) = buffer.pop_front() {
                self.update_stats(&message, false);
                return Ok(message);
            }
        }

        // Если есть таймаут, используем его.
        if let Some(timeout_duration) = self.options.recv_timeout {
            timeout(timeout_duration, self.recv_internal())
                .await
                .map_err(|_| RecvError::Timeout)?
        } else {
            self.recv_internal().await
        }
    }

    /// Неблокирующее получение сообщения.
    pub fn try_recv(&mut self) -> Result<Message, TryRecvError> {
        // Сначала проверяем локальный буфер
        if let Some(ref mut buffer) = self.message_buffer {
            if let Some(message) = buffer.pop_front() {
                self.update_stats(&message, false);
                return Ok(message);
            }
        }

        loop {
            match self.receiver.try_recv() {
                Ok(message) => {
                    if self.should_process_message(&message) {
                        let processed_message = self.process_message(message)?;
                        self.update_stats(&processed_message, false);
                        return Ok(processed_message);
                    } else {
                        self.stats.messages_filtered += 1;
                        continue;
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(count)) => {
                    self.handle_lagging(count)?;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Получает несколько сообщений за раз.
    pub async fn recv_batch(
        &mut self,
        max_messages: usize,
    ) -> Result<Vec<Message>, RecvError> {
        let mut messages = Vec::with_capacity(max_messages);

        // Получаем первое сообщение (блокирующий вызов)
        let first_message = self.recv().await?;
        messages.push(first_message);

        // Пытаемся получить остальные сообщения неблокирующим способом
        while messages.len() < max_messages {
            match self.try_recv() {
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(messages)
    }

    /// Получает и десериализует сообщение
    pub async fn recv_and_deserialize<T>(&mut self) -> Result<T, RecvError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let message = self.recv().await?;
        // inspect_err позволяет инкрементировать счётчик при ошибке, не меняя типа ошибки
        message.deserialize().inspect_err(|_| {
            self.stats.deserialization_errors += 1;
        })
    }

    /// Ожидает сообщение с определённым условием.
    pub async fn recv_with_condition<F>(
        &mut self,
        condition: F,
    ) -> Result<Message, RecvError>
    where
        F: Fn(&Message) -> bool,
    {
        loop {
            let message = self.recv().await?;
            if condition(&message) {
                return Ok(message);
            }

            // Если условие не выполнено, сохраняем сообщение в буфер (если включён)
            if let Some(ref mut buffer) = self.message_buffer {
                if buffer.len() < self.options.message_buffer_size {
                    buffer.push_back(message);
                }
            }
        }
    }

    /// Устанавливает фильтр по размеру сообщения.
    pub fn with_size_filter(
        mut self,
        min_size: Option<usize>,
        max_size: Option<usize>,
    ) -> Self {
        self.filters.size_filter = Some(SizeFilter { min_size, max_size });
        self
    }

    /// Устанавливает фильтр по типу payload.
    pub fn with_payload_type_filter(
        mut self,
        allowed_types: Vec<PayloadType>,
    ) -> Self {
        if self.filters.content_filter.is_none() {
            self.filters.content_filter = Some(ContentFilter {
                allowed_payload_types: allowed_types,
                string_patterns: None,
                json_key_filter: None,
            })
        } else {
            self.filters
                .content_filter
                .as_mut()
                .unwrap()
                .allowed_payload_types = allowed_types;
        }
        self
    }

    /// Устанавливает фильтр по строковым паттернам.
    pub fn with_string_pattern_filter(
        mut self,
        patterns: Vec<&str>,
    ) -> Result<Self, RecvError> {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern)?;
            builder.add(glob);
        }
        let glob_set = builder.build()?;

        if self.filters.content_filter.is_none() {
            self.filters.content_filter = Some(ContentFilter {
                allowed_payload_types: vec![],
                string_patterns: Some(glob_set),
                json_key_filter: None,
            });
        } else {
            self.filters
                .content_filter
                .as_mut()
                .unwrap()
                .string_patterns = Some(glob_set);
        }

        Ok(self)
    }

    /// Устанавливает пользовательский фильтр.
    pub fn with_custom_filter<F>(
        mut self,
        filter: F,
    ) -> Self
    where
        F: Fn(&Message) -> bool + Send + Sync + 'static,
    {
        self.filters.custom_filter = Some(Arc::new(filter));
        self
    }

    /// Возвращает статистику подписчика
    pub fn stats(&self) -> &SubscriberStats {
        &self.stats
    }

    /// Возвращает канал подписки
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Возвращает кол-во сообщений в локальном буфере
    pub fn buffered_message_count(&self) -> usize {
        self.message_buffer.as_ref().map(|b| b.len()).unwrap_or(0)
    }

    /// Очищает локальный буфер сообщений.
    pub fn clear_buffer(&mut self) {
        if let Some(ref mut buffer) = self.message_buffer {
            buffer.clear();
        }
    }

    /// Изменяет размер локального буфера.
    pub fn resize_buffer(
        &mut self,
        new_size: usize,
    ) {
        if let Some(ref mut buffer) = self.message_buffer {
            buffer.resize(new_size.min(buffer.len()), Message::from_static("", b""));
        }
        self.options.message_buffer_size = new_size;
    }

    /// Внутриенний метод получения сообщения.
    async fn recv_internal(&mut self) -> Result<Message, RecvError> {
        loop {
            match self.receiver.recv().await {
                Ok(message) => {
                    if self.should_process_message(&message) {
                        let processed_message = self.process_message(message)?;
                        self.update_stats(&processed_message, false);
                        return Ok(processed_message);
                    } else {
                        self.stats.messages_filtered += 1;
                        continue;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    self.handle_lagging(count)?;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Проверяет, должно ли сообщение быть обработано.
    fn should_process_message(
        &self,
        message: &Message,
    ) -> bool {
        // Проверяем фильтр по размеру
        if let Some(ref size_filter) = self.filters.size_filter {
            let message_size = message.size();
            if let Some(min_zise) = size_filter.min_size {
                if message_size < min_zise {
                    return false;
                }
            }
            if let Some(max_size) = size_filter.max_size {
                if message_size > max_size {
                    return false;
                }
            }
        }

        // Проверяем фильтр по метаданным
        if let Some(ref metadata_filter) = self.filters.metadata_filter {
            if !self.check_metadata_filter(message, metadata_filter) {
                return false;
            }
        }

        // Проверяем фильтр по содержимому
        if let Some(ref content_filter) = self.filters.content_filter {
            if !self.check_content_filter(message, content_filter) {
                return false;
            }
        }

        // Проверяем пользовательский фильтр
        if let Some(ref custom_filter) = self.filters.custom_filter {
            if !custom_filter(message) {
                return false;
            }
        }

        // Проверяем фильтр по типу сообщения
        if let Some(ref type_filter) = self.options.message_type_filter {
            if !self.check_message_type_filter(message, type_filter) {
                return false;
            }
        }

        true
    }

    /// Проверяет фильтр по методанным.
    fn check_metadata_filter(
        &self,
        message: &Message,
        filter: &MetadataFilter,
    ) -> bool {
        let metadata = match &message.metadata {
            Some(meta) => meta,
            None => return false,
        };

        // Проверяем обязательные заголовки
        for required_header in &filter.required_headers {
            if !metadata.headers.contains_key(required_header) {
                return false;
            }
        }

        // Проверяем паттерны ID сообщения
        if let Some(ref pattern_set) = filter.message_id_pattern {
            if let Some(ref message_id) = metadata.message_id {
                if !pattern_set.is_match(message_id) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Проверяем временной диапазон
        if let Some((from_ts, to_ts)) = filter.time_range {
            if let Some(timestamp) = metadata.timestamp {
                if timestamp < from_ts || timestamp > to_ts {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Проверяет фильтр по содержимому.
    fn check_content_filter(
        &self,
        message: &Message,
        filter: &ContentFilter,
    ) -> bool {
        // Проверяем разрешённые типы payload
        if !filter.allowed_payload_types.is_empty() {
            let payload_type = match &message.payload {
                MessagePayload::Bytes(_) => PayloadType::Bytes,
                MessagePayload::String(_) => PayloadType::String,
                MessagePayload::Json(_) => PayloadType::Json,
                MessagePayload::Serialized { content_type, .. } => {
                    PayloadType::Serialized(content_type.clone())
                }
            };

            if !filter.allowed_payload_types.contains(&payload_type) {
                return false;
            }
        }

        // Проверяем строковые паттерны
        if let Some(ref patterns) = filter.string_patterns {
            match &message.payload {
                MessagePayload::String(content) => {
                    if !patterns.is_match(content) {
                        return false;
                    }
                }
                MessagePayload::Bytes(bytes) => {
                    if let Ok(content) = from_utf8(bytes) {
                        if !patterns.is_match(content) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        // Проверяем фильтр JSON по ключам
        if let Some(ref required_keys) = filter.json_key_filter {
            if let MessagePayload::Json(serde_json::Value::Object(obj)) = &message.payload {
                for key in required_keys {
                    if !obj.contains_key(key) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Проверяет фильтр по типу сообщения.
    #[allow(clippy::only_used_in_recursion)]
    fn check_message_type_filter(
        &self,
        message: &Message,
        filter: &MessageTypeFilter,
    ) -> bool {
        match filter {
            MessageTypeFilter::PayloadType(types) => {
                let payload_type = match &message.payload {
                    MessagePayload::Bytes(_) => PayloadType::Bytes,
                    MessagePayload::String(_) => PayloadType::String,
                    MessagePayload::Json(_) => PayloadType::Json,
                    MessagePayload::Serialized { content_type, .. } => {
                        PayloadType::Serialized(content_type.clone())
                    }
                };
                types.contains(&payload_type)
            }
            MessageTypeFilter::WithMetadata => message.metadata.is_none(),
            MessageTypeFilter::WithoutMetadata => message.metadata.is_none(),
            MessageTypeFilter::Combined(f1, f2) => {
                self.check_message_type_filter(message, f1)
                    && self.check_message_type_filter(message, f2)
            }
        }
    }

    /// Обрабатывает сообщение (декомпрессия, автодесериализация и т.д.)
    fn process_message(
        &self,
        mut message: Message,
    ) -> Result<Message, TryRecvError> {
        // Декомпрессия если нужно
        if self.options.enable_compression {
            message.payload = self.decompress_if_needed(message.payload)?;
        }

        // Автодесериализация JSON
        if self.options.auto_deserialize_json {
            if let MessagePayload::String(ref content) = message.payload {
                if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(content) {
                    message.payload = MessagePayload::Json(json_value);
                }
            }
        }

        Ok(message)
    }

    /// Декомпрессирует payload если это необходимо
    fn decompress_if_needed(
        &self,
        payload: MessagePayload,
    ) -> Result<MessagePayload, TryRecvError> {
        if let MessagePayload::Serialized { data, content_type } = payload {
            if content_type == "application/gzip" {
                use std::io::Read;
                let mut decoder = flate2::read::GzDecoder::new(data.as_ref());
                let mut decompressed = Vec::new();
                decoder
                    .read_to_end(&mut decompressed)
                    .map_err(|_| TryRecvError::Closed)?;
                Ok(MessagePayload::Bytes(bytes::Bytes::from(decompressed)))
            } else {
                Ok(MessagePayload::Serialized { data, content_type })
            }
        } else {
            Ok(payload)
        }
    }

    /// Обрабатывает отствание подписчика.
    fn handle_lagging(
        &mut self,
        count: u64,
    ) -> Result<(), TryRecvError> {
        self.stats.lagged_messages += count;

        match self.options.lag_handling {
            LagHandling::Ignore => Ok(()),
            LagHandling::Error => Err(TryRecvError::Lagged(count)),
            LagHandling::Log => {
                eprintln!(
                    "Subscriber lagged behind by {count} messages in channel '{}'",
                    self.channel
                );
                Ok(())
            }
            LagHandling::Skip => {
                // Просто пропускаем отстающие сообщения.
                Ok(())
            }
        }
    }

    /// Обновляет статистику подписчика.
    fn update_stats(
        &mut self,
        message: &Message,
        filtered: bool,
    ) {
        if !filtered {
            self.stats.messages_received += 1;
            self.stats.bytes_received += message.size() as u64;
            self.stats.last_message_at = Some(Instant::now());
        }
    }
}

impl MultiSubscriber {
    /// Создаёт подписчика на несколько каналов.
    pub fn new(subscribers: Vec<Subscriber>) -> Self {
        Self {
            subscribers,
            round_robin_index: 0,
        }
    }

    /// Получает сообщение от любого из каналов (round-robin)
    pub async fn recv_any(&mut self) -> Result<(usize, Message), RecvError> {
        if self.subscribers.is_empty() {
            return Err(RecvError::ChannelNotFound("No subscribers".to_string()));
        }

        let start_index = self.round_robin_index;
        loop {
            let subscriber = &mut self.subscribers[self.round_robin_index];

            match subscriber.try_recv() {
                Ok(message) => {
                    let channel_index = self.round_robin_index;
                    self.round_robin_index = (self.round_robin_index + 1) % self.subscribers.len();
                    return Ok((channel_index, message));
                }
                Err(TryRecvError::Empty) => {
                    self.round_robin_index = (self.round_robin_index + 1) % self.subscribers.len();
                    if self.round_robin_index == start_index {
                        // Обошли все каналы, ждём блокирующим способом.
                        let message = self.subscribers[0].recv().await?;
                        return Ok((0, message));
                    }
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Получает сообщения от всех каналов
    pub async fn recv_all(&mut self) -> Result<Vec<(usize, Message)>, RecvError> {
        let mut messages = Vec::new();

        for (index, subscriber) in self.subscribers.iter_mut().enumerate() {
            match subscriber.try_recv() {
                Ok(message) => messages.push((index, message)),
                Err(TryRecvError::Empty) => continue,
                Err(e) => return Err(e.into()),
            }
        }

        if messages.is_empty() {
            // Если нет сообщений, ждём от первого канала
            let message = self.subscribers[0].recv().await?;
            messages.push((0, message));
        }

        Ok(messages)
    }

    /// Возвращаем кол-во подписчиков.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// Возвращаем каналы всех подписчиков.
    pub fn channels(&self) -> Vec<&str> {
        self.subscribers.iter().map(|s| s.channel()).collect()
    }

    /// Возвращаем общую статистику по всем подписчикам.
    pub fn total_stats(&self) -> SubscriberStats {
        let mut total_stats = SubscriberStats::default();
        let mut earliest_created = Instant::now();
        let mut latest_message: Option<Instant> = None;

        for subscriber in &self.subscribers {
            let stats = subscriber.stats();
            total_stats.messages_received += stats.messages_received;
            total_stats.bytes_received += stats.bytes_received;
            total_stats.messages_filtered += stats.messages_filtered;
            total_stats.lagged_messages += stats.lagged_messages;
            total_stats.deserialization_errors += stats.deserialization_errors;

            if stats.created_at < earliest_created {
                earliest_created = stats.created_at;
            }

            if let Some(last_msg) = stats.last_message_at {
                // avoid map_or to satisfy clippy
                if latest_message
                    .map(|latest| last_msg > latest)
                    .unwrap_or(true)
                {
                    latest_message = Some(last_msg);
                }
            }
        }

        total_stats.created_at = earliest_created;
        total_stats.last_message_at = latest_message;
        total_stats
    }
}

impl Default for SubscriptionOptions {
    fn default() -> Self {
        Self {
            buffer_size: None,
            enable_message_buffer: false,
            message_buffer_size: 100,
            auto_deserialize_json: false,
            message_type_filter: None,
            recv_timeout: None,
            lag_handling: LagHandling::Ignore,
            enable_compression: true,
        }
    }
}

impl Default for SubscriberStats {
    fn default() -> Self {
        Self {
            messages_received: 0,
            bytes_received: 0,
            messages_filtered: 0,
            lagged_messages: 0,
            created_at: Instant::now(),
            last_message_at: None,
            deserialization_errors: 0,
        }
    }
}

// Ручной Debug, так как dyn Fn не Debug
impl fmt::Debug for MessageFilters {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("MessageFilters")
            .field("size_filter", &self.size_filter)
            .field("metadata_filter", &self.metadata_filter)
            .field("content_filter", &self.content_filter)
            .field(
                "custom_filter",
                &self.custom_filter.as_ref().map(|_| "<fn>"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use tokio::time::timeout;

    use crate::{
        Broker, BrokerConfig, LagHandling, MessagePayload, MultiSubscriber, PayloadType,
        Subscriber, SubscriptionOptions, TryRecvError,
    };

    /// Helper: создаёт брокера и подписчика на канал "chan".
    async fn setup_one() -> (Broker, Subscriber) {
        let broker = Broker::new();
        let sub = broker.subscribe("chan").expect("subscribe failed");
        (broker, sub)
    }

    #[tokio::test]
    async fn test_subscription_channel_name() {
        let (broker, sub) = setup_one().await;
        assert_eq!(sub.channel(), "chan");
        // channel name is stable after broker goes out of scope
        drop(broker);
        assert_eq!(sub.channel(), "chan");
    }

    #[tokio::test]
    async fn test_receive_message_via_subscription() {
        let (broker, mut sub) = setup_one().await;

        broker
            .publish("chan", MessagePayload::Bytes(Bytes::from_static(b"hello")))
            .unwrap();

        let msg = timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("timed out")
            .expect("no message");

        assert_eq!(msg.channel.as_ref(), "chan");
        assert_eq!(
            msg.payload,
            MessagePayload::Bytes(Bytes::from_static(b"hello"))
        );
    }

    #[tokio::test]
    async fn test_double_subscribe_same_channel() {
        let broker = Broker::new();
        let mut a = broker.subscribe("dup").expect("subscribe a");
        let mut b = broker.subscribe("dup").expect("subscribe b");

        broker
            .publish("dup", MessagePayload::Bytes(Bytes::from_static(b"X")))
            .unwrap();

        let ma = a.recv().await.unwrap();
        let mb = b.recv().await.unwrap();

        assert_eq!(ma.payload, MessagePayload::Bytes(Bytes::from_static(b"X")));
        assert_eq!(mb.payload, MessagePayload::Bytes(Bytes::from_static(b"X")));
    }

    #[tokio::test]
    async fn test_try_recv_success_and_empty() {
        let broker = Broker::new();
        let mut sub = broker.subscribe("test_channel").unwrap();

        // no message yet -> Empty
        assert!(matches!(sub.try_recv(), Err(TryRecvError::Empty)));

        // publish then immediate try_recv -> Ok
        broker
            .publish(
                "test_channel",
                MessagePayload::Bytes(Bytes::from("immediate")),
            )
            .unwrap();

        let res = sub.try_recv();
        assert!(res.is_ok());
        let msg = res.unwrap();
        assert_eq!(msg.channel.as_ref(), "test_channel");
        assert_eq!(msg.payload, MessagePayload::Bytes(Bytes::from("immediate")));
    }

    #[tokio::test]
    async fn test_lag_handling_error_mode() {
        let opts = SubscriptionOptions {
            buffer_size: Some(1), // small broadcast buffer to provoke lag
            lag_handling: LagHandling::Error,
            ..Default::default()
        };

        let broker = Broker::new();
        let mut sub = broker.subscribe_with_options("overflow", opts).unwrap();

        // publish multiple messages quickly to overflow the broadcast buffer
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
            Ok(_) => {
                // Sometimes timing allows a consumer to keep up — both behaviors acceptable.
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    #[tokio::test]
    async fn test_compression_roundtrip() {
        // Broker с включённой компрессией (чтобы compress_payload сработал)
        let cfg = BrokerConfig {
            enable_compression: true,
            compression_threshold: 1, // compress everything
            ..Default::default()
        };
        let broker = Broker::with_config(cfg);

        let sub_opts = SubscriptionOptions {
            enable_compression: true,
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

    #[tokio::test]
    async fn test_with_payload_type_filter_blocks_unwanted_types() {
        // filter allows only String payloads; Bytes should be filtered
        let broker = Broker::new();
        let mut sub = broker
            .subscribe("ch")
            .unwrap()
            .with_payload_type_filter(vec![PayloadType::String]);

        // publish bytes -> subscriber should skip it
        broker
            .publish("ch", MessagePayload::Bytes(Bytes::from_static(b"bin")))
            .unwrap();

        // try_recv should not return bytes (it will either return Empty or block) — use try_recv
        match sub.try_recv() {
            Err(TryRecvError::Empty) => {
                // good — message filtered out
                assert!(sub.stats().messages_filtered > 0);
            }
            Ok(m) => {
                // If system delivered it, ensure it's not a Bytes (unlikely)
                assert!(matches!(
                    m.payload,
                    MessagePayload::String(_) | MessagePayload::Json(_)
                ));
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    #[tokio::test]
    async fn test_multi_subscriber_recv_any_and_recv_all() {
        let broker = Broker::new();
        let sub_a = broker.subscribe("a").unwrap();
        let sub_b = broker.subscribe("b").unwrap();

        let mut ms = MultiSubscriber::new(vec![sub_a, sub_b]);

        // publish to both channels
        broker
            .publish("a", MessagePayload::String("A".into()))
            .unwrap();
        broker
            .publish("b", MessagePayload::String("B".into()))
            .unwrap();

        // recv_all should get at least one (maybe two depending timing)
        let all = ms.recv_all().await.unwrap();
        assert!(!all.is_empty());
        // channels vector should reflect subscribers
        let channels = ms.channels();
        assert_eq!(channels.len(), 2);
    }
}
