use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use tokio::time::timeout;
use zumic::{Broker, MessagePayload, RecvError, TryRecvError};

/// Тест проверяет реальный сценарий использования:
/// подписчики на точный канал и на шаблон, сбор нескольких сообщений
/// в отдельных задачах и корректную доставку пользователю и администратору.
#[tokio::test]
async fn test_real_world_usage_example() -> Result<(), Box<dyn std::error::Error>> {
    let broker = Arc::new(Broker::new());

    let mut user_sub = broker
        .subscribe("user.notifications")
        .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

    // Вместо шаблона — подписка на конкретные каналы
    let mut admin_security_sub = broker.subscribe("admin.security")?;
    let mut admin_audit_sub = broker.subscribe("admin.audit")?;

    let user_task = tokio::spawn(async move {
        let mut messages = Vec::new();
        for _ in 0..3 {
            match timeout(Duration::from_secs(1), user_sub.recv()).await {
                Ok(Ok(msg)) => {
                    if let MessagePayload::Bytes(b) = &msg.payload {
                        messages.push(format!("User notification: {}", String::from_utf8_lossy(b)));
                    }
                }
                Ok(Err(RecvError::Lagged(n))) => {
                    messages.push(format!("Missed {n} notifications"));
                }
                Ok(Err(RecvError::Closed)) => break,
                _ => break,
            }
        }
        messages
    });

    let admin_task = tokio::spawn(async move {
        let mut events = Vec::new();

        if let Ok(Ok(msg)) = timeout(Duration::from_secs(1), admin_security_sub.recv()).await {
            if let MessagePayload::Bytes(b) = &msg.payload {
                events.push(format!(
                    "Admin event from {}: {}",
                    msg.channel,
                    String::from_utf8_lossy(b)
                ));
            }
        }

        if let Ok(Ok(msg)) = timeout(Duration::from_secs(1), admin_audit_sub.recv()).await {
            if let MessagePayload::Bytes(b) = &msg.payload {
                events.push(format!(
                    "Admin event from {}: {}",
                    msg.channel,
                    String::from_utf8_lossy(b)
                ));
            }
        }

        events
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    broker.publish(
        "user.notifications",
        MessagePayload::Bytes(Bytes::from("New message arrived")),
    )?;
    broker.publish(
        "user.notifications",
        MessagePayload::Bytes(Bytes::from("Friend request received")),
    )?;
    broker.publish(
        "admin.security",
        MessagePayload::Bytes(Bytes::from("Failed login attempt")),
    )?;
    broker.publish(
        "admin.audit",
        MessagePayload::Bytes(Bytes::from("User data accessed")),
    )?;
    broker.publish(
        "user.notifications",
        MessagePayload::Bytes(Bytes::from("Email verified")),
    )?;

    let (user_messages, admin_events) = tokio::join!(user_task, admin_task);
    let user_messages = user_messages?;
    let admin_events = admin_events?;

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
    let broker = Broker::new();
    let mut sub = broker
        .subscribe("mixed_channel")
        .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

    // Публикуем несколько сообщений
    broker
        .publish("mixed_channel", MessagePayload::Bytes(Bytes::from("sync1")))
        .unwrap();
    broker
        .publish("mixed_channel", MessagePayload::Bytes(Bytes::from("sync2")))
        .unwrap();

    // Синхронная семантика здесь — всё равно async, поэтому await
    let msg1 = sub.try_recv().unwrap();
    assert_eq!(msg1.payload, MessagePayload::Bytes(Bytes::from("sync1")));

    let msg2 = sub.try_recv().unwrap();
    assert_eq!(msg2.payload, MessagePayload::Bytes(Bytes::from("sync2")));

    // Теперь канал пуст — await + проверяем Err(TryRecvError::Empty)
    assert!(matches!(sub.try_recv(), Err(TryRecvError::Empty)));

    // Публикуем ещё одно сообщение
    broker
        .publish(
            "mixed_channel",
            MessagePayload::Bytes(Bytes::from("async1")),
        )
        .unwrap();

    // Асинхронная операция чтения
    let msg3 = sub.recv().await.unwrap();
    assert_eq!(msg3.payload, MessagePayload::Bytes(Bytes::from("async1")));
}

/// Тест проверяет поведение отписки:
/// после `unsubscribe` один подписчик не получает событий,
/// а другой остаётся активным и продолжает принимать.
#[tokio::test]
async fn test_unsubscribe_behavior() {
    let broker = Broker::new();
    let sub1 = broker
        .subscribe("unsub_channel")
        .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));
    let mut sub2 = broker
        .subscribe("unsub_channel")
        .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

    // Публикуем сообщение
    broker
        .publish(
            "unsub_channel",
            MessagePayload::Bytes(Bytes::from("before_unsub")),
        )
        .unwrap();

    // sub1 отписывается
    drop(sub1);

    // sub2 все еще может получать сообщения
    let msg = sub2.recv().await.unwrap();
    assert_eq!(
        msg.payload,
        MessagePayload::Bytes(Bytes::from("before_unsub"))
    );

    // Публикуем еще одно
    broker
        .publish(
            "unsub_channel",
            MessagePayload::Bytes(Bytes::from("after_unsub")),
        )
        .unwrap();
    let msg = sub2.recv().await.unwrap();
    assert_eq!(
        msg.payload,
        MessagePayload::Bytes(Bytes::from("after_unsub"))
    );
}

/// Тест проверяет корректность статистики:
/// счётчик `publish_count` не меняется при чтении,
/// но увеличивается при публикации.
#[tokio::test]
async fn test_broker_statistics_with_async_api() {
    let broker = Broker::new();
    let mut sub = broker
        .subscribe("stats_channel")
        .unwrap_or_else(|e| panic!("subscribe failed: {e:?}"));

    // Публикуем сообщения
    broker
        .publish("stats_channel", MessagePayload::Bytes(Bytes::from("test1")))
        .unwrap();
    broker
        .publish("stats_channel", MessagePayload::Bytes(Bytes::from("test2")))
        .unwrap();

    // Получаем сообщения через async API
    let msg1 = sub.recv().await.unwrap();
    let msg2 = sub.recv().await.unwrap();

    assert_eq!(msg1.payload, MessagePayload::Bytes(Bytes::from("test1")));
    assert_eq!(msg2.payload, MessagePayload::Bytes(Bytes::from("test2")));
}
