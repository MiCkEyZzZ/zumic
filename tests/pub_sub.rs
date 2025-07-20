use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use bytes::Bytes;

use zumic::{Broker, RecvError, TryRecvError};

/// Тест проверяет реальный сценарий использования:
/// подписчики на точный канал и на шаблон, сбор нескольких сообщений
/// в отдельных задачах и корректную доставку пользователю и администратору.
#[tokio::test]
async fn test_real_world_usage_example() -> Result<(), Box<dyn std::error::Error>> {
    let broker = Arc::new(Broker::new(100));

    // Подписываемся на уведомления о пользователях
    let mut user_sub = broker.subscribe("user.notifications");
    let mut admin_pattern = broker.psubscribe("admin.*")?;

    // Создаем задачу для обработки пользовательских уведомлений
    let _broker_clone = broker.clone();
    let user_task = tokio::spawn(async move {
        let mut messages = Vec::new();

        // Обрабатываем до 3 сообщений
        for _ in 0..3 {
            match user_sub.recv().await {
                Ok(msg) => {
                    messages.push(format!(
                        "User notification: {}",
                        String::from_utf8_lossy(&msg.payload)
                    ));
                }
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(n)) => {
                    messages.push(format!("Missed {} notifications", n));
                }
            }
        }
        messages
    });

    // Создаем задачу для обработки админских событий
    let admin_task = tokio::spawn(async move {
        let mut events = Vec::new();

        // Обрабатываем до 2 событий
        for _ in 0..2 {
            match admin_pattern.recv().await {
                Ok(msg) => {
                    events.push(format!(
                        "Admin event from {}: {}",
                        msg.channel,
                        String::from_utf8_lossy(&msg.payload)
                    ));
                }
                Err(_) => break,
            }
        }
        events
    });

    // Даём подписчикам время подключиться
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Публикуем события
    broker.publish("user.notifications", Bytes::from("New message arrived"));
    broker.publish("user.notifications", Bytes::from("Friend request received"));
    broker.publish("admin.security", Bytes::from("Failed login attempt"));
    broker.publish("admin.audit", Bytes::from("User data accessed"));
    broker.publish("user.notifications", Bytes::from("Email verified"));

    // Ждем результатов
    let (user_messages, admin_events) = tokio::join!(user_task, admin_task);

    let user_messages = user_messages?;
    let admin_events = admin_events?;

    // Проверяем результаты
    assert_eq!(user_messages.len(), 3);
    assert!(user_messages[0].contains("New message arrived"));
    assert!(user_messages[1].contains("Friend request received"));
    assert!(user_messages[2].contains("Email verified"));

    assert_eq!(admin_events.len(), 2);
    assert!(admin_events[0].contains("admin.security"));
    assert!(admin_events[1].contains("admin.audit"));

    Ok(())
}

/// Тест проверяет смешанное синхронное и асинхронное чтение:
/// `try_recv().await` для немедленного получения из буфера
/// и `recv().await` для ожидания следующего события.
#[tokio::test]
async fn test_mixed_sync_async_usage() {
    let broker = Broker::new(10);
    let mut sub = broker.subscribe("mixed_channel");

    // Публикуем несколько сообщений
    broker.publish("mixed_channel", Bytes::from("sync1"));
    broker.publish("mixed_channel", Bytes::from("sync2"));

    // Синхронная семантика здесь — всё равно async, поэтому await
    let msg1 = sub.try_recv().await.unwrap();
    assert_eq!(msg1.payload, Bytes::from("sync1"));

    let msg2 = sub.try_recv().await.unwrap();
    assert_eq!(msg2.payload, Bytes::from("sync2"));

    // Теперь канал пуст — await + проверяем Err(TryRecvError::Empty)
    assert!(matches!(sub.try_recv().await, Err(TryRecvError::Empty)));

    // Публикуем ещё одно сообщение
    broker.publish("mixed_channel", Bytes::from("async1"));

    // Асинхронная операция чтения
    let msg3 = sub.recv().await.unwrap();
    assert_eq!(msg3.payload, Bytes::from("async1"));
}

/// Тест проверяет поведение отписки:
/// после `unsubscribe` один подписчик не получает событий,
/// а другой остаётся активным и продолжает принимать.
#[tokio::test]
async fn test_unsubscribe_behavior() {
    let broker = Broker::new(10);
    let sub1 = broker.subscribe("unsub_channel");
    let mut sub2 = broker.subscribe("unsub_channel");

    // Публикуем сообщение
    broker.publish("unsub_channel", Bytes::from("before_unsub"));

    // sub1 отписывается
    sub1.unsubscribe();

    // sub2 все еще может получать сообщения
    let msg = sub2.recv().await.unwrap();
    assert_eq!(msg.payload, Bytes::from("before_unsub"));

    // Публикуем еще одно
    broker.publish("unsub_channel", Bytes::from("after_unsub"));
    let msg = sub2.recv().await.unwrap();
    assert_eq!(msg.payload, Bytes::from("after_unsub"));
}

/// Тест проверяет корректность статистики:
/// счётчик `publish_count` не меняется при чтении,
/// но увеличивается при публикации.
#[tokio::test]
async fn test_broker_statistics_with_async_api() {
    let broker = Broker::new(10);
    let mut sub = broker.subscribe("stats_channel");

    assert_eq!(broker.publish_count.load(Ordering::Relaxed), 0);

    // Публикуем и получаем сообщения
    broker.publish("stats_channel", Bytes::from("test1"));
    broker.publish("stats_channel", Bytes::from("test2"));

    assert_eq!(broker.publish_count.load(Ordering::Relaxed), 2);

    // Получаем сообщения через новый API
    let _msg1 = sub.recv().await.unwrap();
    let _msg2 = sub.recv().await.unwrap();

    // Статистика должна остаться корректной
    assert_eq!(broker.publish_count.load(Ordering::Relaxed), 2);
}
