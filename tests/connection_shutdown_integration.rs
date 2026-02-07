use std::{sync::Arc, time::Duration};

use anyhow::Result;
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    time::sleep,
};
use zumic::{
    network::connection::{ConnectionConfig, ConnectionManager},
    InMemoryStore, StorageEngine,
};

#[tokio::test(flavor = "current_thread")]
#[allow(clippy::arc_with_non_send_sync)] // engine (InMemoryStore) is not Send/Sync yet; test runs on current_thread
async fn connection_handler_shutdown_notify() -> Result<()> {
    // Конфиг с короткими таймаутами для теста
    let cfg = ConnectionConfig {
        read_timeout: Duration::from_secs(5),
        write_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(300),
        ..Default::default()
    };

    let engine = Arc::new(StorageEngine::Memory(InMemoryStore::new()));
    let manager = Arc::new(ConnectionManager::new(cfg.clone()));

    // Listener на свободном порту
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let local_addr = listener.local_addr()?;

    // Серверная future: accept -> handle_connection
    let manager_server = manager.clone();
    let engine_server = engine.clone();
    let server_fut = async move {
        let (socket, addr) = listener.accept().await?;
        // Запускаем обработку (в этом же потоке)
        manager_server
            .handle_connection(socket, addr, engine_server)
            .await?;
        Ok::<(), anyhow::Error>(())
    };

    // Клиентская future: подключиться, подождать, вызвать shutdown у менеджера,
    // проверить сообщение
    let manager_client = manager.clone();
    let client_fut = async move {
        let mut client = TcpStream::connect(local_addr).await?;
        // Небольшая пауза, чтобы сервер успел попасть в select/loop
        sleep(Duration::from_millis(50)).await;

        // Посылаем сигнал shutdown — менеджер уведомит handler'ы через Notify
        manager_client.shutdown();

        // Читаем ответ от сервера — он должен послать "-ERR Server shutting down\r\n"
        let mut buf = vec![0u8; 256];
        // Подождём немного, пока ответ придёт
        sleep(Duration::from_millis(20)).await;
        let n = client.read(&mut buf).await?;
        let got = String::from_utf8_lossy(&buf[..n]).to_string();
        assert!(
            got.contains("Server shutting down") || got.contains("-ERR"),
            "expected shutdown message, got: {got:?}"
        );

        Ok::<(), anyhow::Error>(())
    };

    // Запускаем параллельно (в одном потоке)
    tokio::try_join!(server_fut, client_fut)?;
    Ok(())
}
