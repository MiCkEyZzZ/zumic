use std::{io::ErrorKind, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use tokio::{net::TcpListener, signal, sync::oneshot, task::JoinHandle};
use tracing::{error, info, warn};

use crate::{
    network::connection::{ConnectionConfig, ConnectionManager},
    StorageEngine,
};

/// Конфигурация сервера.
/// Определяет адрес для прослушивания, настройки соединений и таймаут graceful
/// shutdown.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Адрес и порт для прослушивания входящих соединений.
    pub listen_address: std::net::SocketAddr,
    /// Конфигурация соединений (таймауты, лимиты, буферы).
    pub connection_config: ConnectionConfig,
    /// Таймаут для graceful shutdown, сек.
    pub shutdown_timeout: Duration,
}

/// Основной сервер для обработки TCP соединений.
pub struct Server {
    config: ServerConfig,
    connection_manager: Arc<ConnectionManager>,
    engine: Arc<StorageEngine>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_handle: Option<JoinHandle<Result<()>>>,
}

/// Статистика по соединениям.
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    /// Количество активных соединений.
    pub active_connections: usize,
}

impl Server {
    /// Создаёт новый сервер с заданной конфигурацией и движком хранения.
    pub fn new(
        config: ServerConfig,
        engine: Arc<StorageEngine>,
    ) -> Self {
        let connection_manager = Arc::new(ConnectionManager::new(config.connection_config.clone()));

        Self {
            config,
            connection_manager,
            engine,
            shutdown_tx: None,
            server_handle: None,
        }
    }

    /// Запускает сервер и начинает принимать входящие соединения.
    /// Ожидает LocalSet / current-thread runtime.
    pub async fn start(&mut self) -> Result<()> {
        let listener = TcpListener::bind(self.config.listen_address)
            .await
            .context("Failed to bind to address")?;

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        Self::run_server(
            listener,
            self.connection_manager.clone(),
            self.engine.clone(),
            shutdown_rx,
            self.config.shutdown_timeout,
        )
        .await
    }

    /// Graceful остановка сервера.
    /// Ждет завершения активных соединений до указанного таймаута.
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Initiating server shutdown...");

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.server_handle.take() {
            match handle.await {
                Ok(result) => result,
                Err(e) => {
                    error!("Server task panicked: {e}");
                    Err(e.into())
                }
            }
        } else {
            Ok(())
        }
    }

    /// Получение текущей статистики по соединениям.
    pub fn connection_stats(&self) -> ConnectionStats {
        ConnectionStats {
            active_connections: self.connection_manager.active_connections(),
        }
    }

    /// Основной цикл сервера.
    /// Обрабатывает новые соединения и сигналы shutdown.
    async fn run_server(
        listener: TcpListener,
        connection_manager: Arc<ConnectionManager>,
        engine: Arc<StorageEngine>,
        mut shutdown_rx: oneshot::Receiver<()>,
        shutdown_timeout: Duration,
    ) -> Result<()> {
        let mut connection_tasks: Vec<JoinHandle<()>> = Vec::new();

        // На Unix: подготовим обработчики сигналов заранее
        #[cfg(unix)]
        let mut term_signal = signal::unix::signal(signal::unix::SignalKind::terminate())
            .context("failed to create SIGTERM handler")?;

        #[cfg(unix)]
        let mut int_signal = signal::unix::signal(signal::unix::SignalKind::interrupt())
            .context("failed to create SIGINT handler")?;

        // Основной цикл приема соединений
        #[cfg(unix)]
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, addr)) => {
                            info!("Accepting connection from {addr}");

                            let manager = connection_manager.clone();
                            let eng = engine.clone();

                            let task = tokio::task::spawn_local(async move {
                                if let Err(e) = manager.handle_connection(socket, addr, eng).await {
                                    match e.downcast_ref::<std::io::Error>() {
                                        Some(io_err) if Server::is_expected_error(io_err) => {
                                            tracing::debug!("Connection from {addr} ended: {e}");
                                        }
                                        _ => {
                                            error!("Unexpected error handling connection from {addr}: {e}");
                                        }
                                    }
                                }
                            });

                            connection_tasks.push(task);
                            connection_tasks.retain(|task| !task.is_finished());
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {e}");
                            if Server::is_critical_accept_error(&e) {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        }
                    }
                }

                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal, stopping server...");
                    break;
                }

                // Сигналы Unix (prepared above)
                _ = term_signal.recv() => {
                    info!("Received SIGTERM, initiating graceful shutdown...");
                    break;
                }

                _ = int_signal.recv() => {
                    info!("Received SIGINT, initiating graceful shutdown...");
                    break;
                }
            }
        }

        #[cfg(not(unix))]
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, addr)) => {
                            info!("Accepting connection from {addr}");

                            let manager = connection_manager.clone();
                            let eng = engine.clone();

                            let task = tokio::spawn(async move {
                                if let Err(e) = manager.handle_connection(socket, addr, eng).await {
                                    match e.downcast_ref::<std::io::Error>() {
                                        Some(io_err) if Server::is_expected_error(io_err) => {
                                            tracing::debug!("Connection from {addr} ended: {e}");
                                        }
                                        _ => {
                                            error!("Unexpected error handling connection from {addr}: {e}");
                                        }
                                    }
                                }
                            });

                            connection_tasks.push(task);
                            connection_tasks.retain(|task| !task.is_finished());
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {e}");
                            if Server::is_critical_accept_error(&e) {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        }
                    }
                }

                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal, stopping server...");
                    break;
                }
            }
        }

        // Graceful shutdown
        info!("Stopping server, waiting for active connections to finish...");

        connection_manager.shutdown();

        if let Err(e) = connection_manager.wait_for_shutdown(shutdown_timeout).await {
            warn!("Not all connections finished gracefully: {e}");
        }

        for task in connection_tasks {
            if !task.is_finished() {
                let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
            }
        }

        info!("Server shutdown completed");
        Ok(())
    }

    /// Проверяет, является ли ошибка закрытия соединения ожидаемой.
    fn is_expected_error(error: &std::io::Error) -> bool {
        matches!(
            error.kind(),
            ErrorKind::UnexpectedEof
                | ErrorKind::BrokenPipe
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::TimedOut
        )
    }

    /// Проверяет, является ли ошибка accept критической.
    fn is_critical_accept_error(error: &std::io::Error) -> bool {
        matches!(
            error.kind(),
            ErrorKind::AddrInUse | ErrorKind::AddrNotAvailable | ErrorKind::PermissionDenied
        )
    }
}

impl Default for ServerConfig {
    /// Создаёт конфигурацию сервера с дефолтными значениями.
    fn default() -> Self {
        Self {
            listen_address: "127.0.0.1:6174".parse().unwrap(),
            connection_config: ConnectionConfig::default(),
            shutdown_timeout: Duration::from_secs(30),
        }
    }
}

/// Устаревшая функция для совместимости, обрабатывает соединение вручную.
/// TODO: удалить после внедрения полноценного Server.
#[deprecated(note = "Use Server struct instead")]
pub async fn handle_connection(
    socket: tokio::net::TcpStream,
    engine: Arc<StorageEngine>,
) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use crate::{Sds, Value};

    let mut lines = BufReader::new(socket).lines();

    loop {
        let line_opt = match lines.next_line().await {
            Ok(Some(line)) => Some(line),
            Ok(None) => break,
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::InvalidData | ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe
                ) =>
            {
                break
            }
            Err(e) => return Err(e.into()),
        };

        let line = line_opt.unwrap();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let response = match parts[0] {
            "SET" if parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let v = Value::Str(Sds::from(parts[2].as_bytes()));
                engine
                    .set(&k, v)
                    .map(|_| "+OK\r\n".to_string())
                    .unwrap_or("-ERR set failed\r\n".to_string())
            }
            "GET" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.get(&k) {
                    Ok(Some(Value::Str(s))) => String::from_utf8(s.to_vec())
                        .map(|s| format!("+{s}\r\n"))
                        .unwrap_or("-ERR utf8\r\n".to_string()),
                    Ok(Some(_)) => "-ERR unsupported type\r\n".to_string(),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(_) => "-ERR get failed\r\n".to_string(),
                }
            }
            "DEL" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.del(&k) {
                    Ok(true) => ":1\r\n".to_string(),
                    Ok(false) => ":0\r\n".to_string(),
                    Err(_) => "-ERR del failed\r\n".to_string(),
                }
            }
            _ => "-ERR unknown command\r\n".to_string(),
        };

        let socket = lines.get_mut();
        socket.write_all(response.as_bytes()).await?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use anyhow::Result;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::oneshot,
        task::LocalSet,
        time::{sleep, timeout, Duration},
    };

    use super::*;
    use crate::InMemoryStore;

    /// Тест проверяет принимает ли соединение, ConnectionHandler отвечает на
    /// PING, затем на QUIT и закрывается.
    #[tokio::test(flavor = "current_thread")]
    async fn server_accepts_connection_and_handles_ping_quit() -> Result<()> {
        // Конфиг — будем вручную создавать listener и передавать в run_server
        let conn_cfg = ConnectionConfig {
            read_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            ..ConnectionConfig::default()
        };

        // Движок и менеджер
        let engine = Arc::new(StorageEngine::Memory(InMemoryStore::new()));
        let manager = Arc::new(ConnectionManager::new(conn_cfg.clone()));

        // listener на свободном порту
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let local_addr: SocketAddr = listener.local_addr()?;

        // oneshot для остановки сервера
        let (tx, rx) = oneshot::channel::<()>();

        // LocalSet для spawn_local (нужно для spawn_local внутри run_server на unix)
        let local = LocalSet::new();

        local
            .run_until(async move {
                // Запускаем сервер как локальную задачу
                let server_task = tokio::task::spawn_local(async move {
                    // Внутренний вызов run_server; вернёт Ok(()) после получения shutdown
                    Server::run_server(
                        listener,
                        manager.clone(),
                        engine.clone(),
                        rx,
                        Duration::from_secs(5),
                    )
                    .await
                    .expect("run_server failed");
                });

                // Небольшая пауза, чтобы сервер успел начать слушать (обычно не обязательна)
                sleep(Duration::from_millis(20)).await;

                // Клиент: подключаемся и проверяем PING/QUIT
                let client_task = async move {
                    let mut stream = TcpStream::connect(local_addr).await?;
                    // PING
                    stream.write_all(b"PING\r\n").await?;
                    let mut buf = vec![0u8; 128];
                    // ждём ответ с таймаутом чтобы не зависнуть в тесте
                    let n = timeout(Duration::from_secs(2), stream.read(&mut buf)).await??;
                    let got = String::from_utf8_lossy(&buf[..n]).to_string();
                    assert!(got.contains("+PONG"), "expected +PONG, got: {got:?}");

                    // QUIT
                    stream.write_all(b"QUIT\r\n").await?;
                    let n = timeout(Duration::from_secs(2), stream.read(&mut buf)).await??;
                    let got = String::from_utf8_lossy(&buf[..n]).to_string();
                    assert!(got.contains("+OK"), "expected +OK, got: {got:?}");

                    Ok::<(), anyhow::Error>(())
                };

                // Выполним клиентскую задачу (в том же потоке)
                client_task.await.expect("client task failed");

                // После выполнения клиента — посылаем shutdown серверу
                let _ = tx.send(());

                // Ждём завершения серверной задачи
                server_task.await.expect("server task panicked");
                Ok::<(), anyhow::Error>(())
            })
            .await?;

        Ok(())
    }

    /// Тест проверяет сервер корректно завершает работу при получении shutdown
    /// через oneshot без входящих соединений.
    #[tokio::test(flavor = "current_thread")]
    async fn server_shutdown_via_channel() -> Result<()> {
        let conn_cfg = ConnectionConfig::default();
        let engine = Arc::new(StorageEngine::Memory(InMemoryStore::new()));
        let manager = Arc::new(ConnectionManager::new(conn_cfg.clone()));

        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let (tx, rx) = oneshot::channel::<()>();

        let local = LocalSet::new();

        local
            .run_until(async move {
                // Запускаем сервер локально
                let server_task = tokio::task::spawn_local(async move {
                    Server::run_server(listener, manager, engine, rx, Duration::from_secs(2))
                        .await
                        .expect("run_server failed");
                });

                // Подождём немного, затем пошлем shutdown
                sleep(Duration::from_millis(50)).await;
                let _ = tx.send(());

                // Ждём, что сервер корректно завершится
                server_task.await.expect("server task panicked");
                Ok::<(), anyhow::Error>(())
            })
            .await?;

        Ok(())
    }
}
