use std::{net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::LocalSet,
    time::sleep,
};
use zumic::{
    network::{
        admin_commands::AdminCommands,
        connection::{ConnectionConfig, ConnectionManager},
        connection_registry::ConnectionRegistry,
        connection_state::{ConnectionInfo, ConnectionState},
    },
    InMemoryStore, StorageEngine,
};

/// Тест проверяет полный lifecycle соединения и административные команды
#[allow(clippy::arc_with_non_send_sync)]
#[tokio::test(flavor = "current_thread")]
async fn test_full_connection_lifecycle_with_admin() -> anyhow::Result<()> {
    let local = LocalSet::new();

    local
        .run_until(async {
            let config = ConnectionConfig {
                read_timeout: Duration::from_secs(5),
                write_timeout: Duration::from_secs(5),
                idle_timeout: Duration::from_secs(60),
                max_connections: 100,
                max_connections_per_ip: 10,
                read_buffer_size: 8192,
            };

            #[allow(clippy::arc_with_non_send_sync)]
            let engine = Arc::new(StorageEngine::Memory(InMemoryStore::new()));
            let manager = Arc::new(ConnectionManager::new(config.clone()));
            let registry = manager.registry().clone();

            // Запускаем тестовый сервер
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let local_addr: SocketAddr = listener.local_addr()?;

            // Серверная задача
            let manager_clone = manager.clone();
            let engine_clone = engine.clone();
            let server_task = tokio::task::spawn_local(async move {
                while let Ok((socket, addr)) = listener.accept().await {
                    let mgr = manager_clone.clone();
                    let eng = engine_clone.clone();

                    tokio::task::spawn_local(async move {
                        let _ = mgr.handle_connection(socket, addr, eng).await;
                    });
                }
            });

            // Даем серверу запуститься
            sleep(Duration::from_millis(50)).await;

            // === Тест 1: Базовое соединение ===
            println!("Test 1: Basic connection");
            let mut client1 = TcpStream::connect(local_addr).await?;

            // Проверяем что соединение зарегистрировано
            sleep(Duration::from_millis(10)).await;
            assert_eq!(registry.active_count(), 1);

            // Отправляем PING
            client1.write_all(b"PING\r\n").await?;
            let mut buf = vec![0u8; 128];
            let n = client1.read(&mut buf).await?;
            let response = String::from_utf8_lossy(&buf[..n]);
            assert!(response.contains("+PONG"), "Expected PONG, got: {response}");

            // === Тест 2: Проверка метаданных ===
            println!("Test 2: Connection metadata");
            let snapshots = registry.all_snapshots();
            assert_eq!(snapshots.len(), 1);

            let snapshot = &snapshots[0];
            assert_eq!(snapshot.state, "idle");
            assert_eq!(snapshot.commands_processed, 1); // 1 PING команда
            assert!(snapshot.bytes_sent > 0);

            // === Тест 3: Множественные соединения ===
            println!("Test 3: Multiple connections");
            let mut client2 = TcpStream::connect(local_addr).await?;
            let mut client3 = TcpStream::connect(local_addr).await?;

            sleep(Duration::from_millis(10)).await;
            assert_eq!(registry.active_count(), 3);

            // Отправляем команды с разных клиентов
            client2.write_all(b"SET key1 value1\r\n").await?;
            let n = client2.read(&mut buf).await?;
            let response = String::from_utf8_lossy(&buf[..n]);
            assert!(response.contains("+OK"));

            client3.write_all(b"GET key1\r\n").await?;
            let n = client3.read(&mut buf).await?;
            let response = String::from_utf8_lossy(&buf[..n]);
            assert!(response.contains("value1"));

            // === Тест 4: Глобальная статистика ===
            println!("Test 4: Global statistics");
            sleep(Duration::from_millis(20)).await;

            let stats = registry.global_stats();
            assert_eq!(stats.active_connections, 3);
            assert!(stats.total_commands >= 3); // PING + SET + GET
            assert!(stats.total_bytes_sent > 0);
            assert!(stats.total_bytes_received > 0);

            println!("Global stats: {stats:#?}");

            // === Тест 5: Фильтрация по IP ===
            println!("Test 5: Filter by IP");
            let ip_snapshots = registry.snapshots_by_ip("127.0.0.1");
            assert_eq!(ip_snapshots.len(), 3);

            // === Тест 6: Административные команды (прямой вызов) ===
            println!("Test 6: Admin commands");
            let admin = AdminCommands::new(registry.clone());

            // CLIENT COUNT
            let count_response = admin.handle_client_count();
            assert!(count_response.contains(":3"));

            // CLIENT LIST
            let list_response = admin.handle_client_list();
            assert!(list_response.starts_with("*3\r\n"));

            // SERVER STATS
            let stats_response = admin.handle_server_stats();
            assert!(stats_response.contains("Active Connections: 3"));
            assert!(stats_response.contains("Total Commands:"));

            println!("Admin stats response:\n{stats_response}");

            // === Тест 7: Закрытие соединения ===
            println!("Test 7: Connection closing");
            client1.write_all(b"QUIT\r\n").await?;
            let n = client1.read(&mut buf).await?;
            let response = String::from_utf8_lossy(&buf[..n]);
            assert!(response.contains("+OK"));

            drop(client1);
            sleep(Duration::from_millis(50)).await;

            // Проверяем что соединение удалено
            assert_eq!(registry.active_count(), 2);

            // === Тест 8: Состояния соединений ===
            println!("Test 8: Connection states");
            let snapshots = registry.all_snapshots();
            for snapshot in &snapshots {
                // Все должны быть в Idle состоянии (кроме Processing во время команды)
                assert!(
                    snapshot.state == "idle" || snapshot.state == "processing",
                    "Unexpected state: {}",
                    snapshot.state
                );
            }

            // === Cleanup ===
            drop(client2);
            drop(client3);
            sleep(Duration::from_millis(50)).await;

            assert_eq!(registry.active_count(), 0);

            server_task.abort();

            println!("All tests passed!");
            Ok::<(), anyhow::Error>(())
        })
        .await?;

    Ok(())
}

/// Тест проверяет что статистика корректно обновляется
#[allow(clippy::arc_with_non_send_sync)]
#[tokio::test(flavor = "current_thread")]
async fn test_connection_statistics_accuracy() -> anyhow::Result<()> {
    let registry = Arc::new(ConnectionRegistry::new());
    let addr: SocketAddr = "127.0.0.1:1234".parse()?;

    let (id1, info1) = registry.register(addr);
    let (id2, info2) = registry.register(addr);

    // Симулируем обработку команд
    info1.record_command(100, 200); // 100 байт получено, 200 отправлено
    info1.record_command(50, 100);
    info2.record_command(75, 150);

    // Проверяем индивидуальную статистику
    let snapshot1 = registry.get_snapshot(id1).unwrap();
    assert_eq!(snapshot1.commands_processed, 2);
    assert_eq!(snapshot1.bytes_received, 150);
    assert_eq!(snapshot1.bytes_sent, 300);

    let snapshot2 = registry.get_snapshot(id2).unwrap();
    assert_eq!(snapshot2.commands_processed, 1);
    assert_eq!(snapshot2.bytes_received, 75);
    assert_eq!(snapshot2.bytes_sent, 150);

    // Проверяем глобальную статистику
    let global = registry.global_stats();
    assert_eq!(global.active_connections, 2);
    assert_eq!(global.total_commands, 3);
    assert_eq!(global.total_bytes_received, 225);
    assert_eq!(global.total_bytes_sent, 450);

    Ok(())
}

/// Тест проверяет работу с состояниями соединений
#[test]
fn test_connection_state_transitions() {
    let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
    let info = Arc::new(ConnectionInfo::new(1, addr));

    // Проверяем переходы состояний
    info.set_state(ConnectionState::New);
    assert_eq!(info.snapshot().state, "new");

    info.set_state(ConnectionState::Idle);
    assert_eq!(info.snapshot().state, "idle");

    info.set_state(ConnectionState::Processing);
    assert_eq!(info.snapshot().state, "processing");

    info.set_state(ConnectionState::Authenticated);
    assert_eq!(info.snapshot().state, "authenticated");

    info.set_state(ConnectionState::Closing);
    assert_eq!(info.snapshot().state, "closing");
}
