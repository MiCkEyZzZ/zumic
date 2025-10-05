use std::{sync::Arc, time::Duration};

use anyhow::Result;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use zumic::{
    network::connection::{ConnectionConfig, ConnectionManager},
    InMemoryStore, StorageEngine,
};

#[tokio::test(flavor = "current_thread")]
#[allow(clippy::arc_with_non_send_sync)]
async fn connection_handler_ping_and_quit() -> Result<()> {
    // Конфиг с короткими таймаутами для теста
    let cfg = ConnectionConfig {
        read_timeout: Duration::from_secs(5),
        write_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(60),
        ..Default::default()
    };

    let engine = Arc::new(StorageEngine::Memory(InMemoryStore::new()));
    let manager = Arc::new(ConnectionManager::new(cfg.clone()));

    // Listener на свободном порту
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let local_addr = listener.local_addr()?;

    // Серверная future: accept -> handle_connection (в текущем потоке)
    let manager_server = manager.clone();
    let engine_server = engine.clone();
    let server_fut = async move {
        let (socket, addr) = listener.accept().await?;
        // Выполняем обработчик в этом же (current_thread) потоке
        manager_server
            .handle_connection(socket, addr, engine_server)
            .await?;
        Ok::<(), anyhow::Error>(())
    };

    // Клиентская future: подключиться, PING, QUIT
    let client_fut = async move {
        let mut client = TcpStream::connect(local_addr).await?;
        client.write_all(b"PING\r\n").await?;
        let mut buf = vec![0u8; 128];
        let n = client.read(&mut buf).await?;
        let got = String::from_utf8_lossy(&buf[..n]);
        assert!(got.contains("+PONG"), "expected +PONG, got {:?}", got);

        client.write_all(b"QUIT\r\n").await?;
        let n = client.read(&mut buf).await?;
        let got = String::from_utf8_lossy(&buf[..n]);
        assert!(got.contains("+OK"), "expected +OK, got {:?}", got);

        Ok::<(), anyhow::Error>(())
    };

    // Запускаем параллельно (в одном потоке) — не требуется Send
    tokio::try_join!(server_fut, client_fut)?;
    Ok(())
}
