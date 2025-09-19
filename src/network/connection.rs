use std::{
    collections::HashMap,
    io::ErrorKind,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedWriteHalf, TcpStream},
    select,
    sync::Semaphore,
    time::{sleep, timeout, Instant},
};
use tracing::{debug, error, info, trace, warn};

use crate::{Sds, StorageEngine, Value};

/// Конфигурация для обработки соединений
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Максимальное кол-во одновременных соединений
    pub max_connections: usize,
    /// Максимальное кол-во соединений с одного IP
    pub max_connections_per_ip: usize,
    /// Таймаут простоя соединения (idle timeout)
    pub idle_timeout: Duration,
    /// Таймаут чтения команды
    pub read_timeout: Duration,
    /// Таймаут записи ответа
    pub write_timeout: Duration,
    /// Размер буфера для чтения
    pub read_buffer_size: usize,
}

/// Менеджер соединений с защитой от DoS и graceful shutdown
#[derive(Debug)]
pub struct ConnectionManager {
    config: ConnectionConfig,
    /// Семафор для ограничений по IP адресам
    connection_semaphore: Arc<Semaphore>,
    /// Счётчик соединений по IP адресам
    ip_connections: Arc<RwLock<HashMap<std::net::IpAddr, AtomicU32>>>,
    /// Общий счётчик активных соединений
    active_connections: Arc<AtomicUsize>,
    /// Флаг для graceful shutdown
    shutdown_signal: Arc<tokio::sync::Notify>,
    /// Счётчик для генерации ID соединений
    connection_counter: Arc<AtomicU32>,
}

/// Обработчик отдельного соединения
pub struct ConnectionHandler {
    connection_id: u32,
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: OwnedWriteHalf,
    addr: SocketAddr,
    engine: Arc<StorageEngine>,
    config: ConnectionConfig,
    shutdown_signal: Arc<tokio::sync::Notify>,
    last_activity: Instant,
}

impl ConnectionManager {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            connection_semaphore: Arc::new(Semaphore::new(config.max_connections)),
            config,
            ip_connections: Arc::new(RwLock::new(HashMap::new())),
            active_connections: Arc::new(AtomicUsize::new(0)),
            shutdown_signal: Arc::new(tokio::sync::Notify::new()),
            connection_counter: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Получение текущее кол-во активных соединений
    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Инициализация graceful shutdown
    pub fn shutdown(&self) {
        info!("Initiating graceful shutdown for connection manager");
        self.shutdown_signal.notify_waiters();
    }

    /// Ждать завершения всех активных соединений
    pub async fn wait_for_shutdown(
        &self,
        timeout_duration: Duration,
    ) -> Result<()> {
        let start = Instant::now();

        while self.active_connections() > 0 {
            if start.elapsed() > timeout_duration {
                warn!(
                    "Shutdown timeout reached with {} active connections",
                    self.active_connections()
                );
                return Err(anyhow!("Shutdown timeout exceeded"));
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("All connection closed gracefully");
        Ok(())
    }

    /// Обрабатывает новое соединение
    pub async fn handle_connection(
        &self,
        socket: TcpStream,
        addr: SocketAddr,
        engine: Arc<StorageEngine>,
    ) -> Result<()> {
        // Проверяем лимиты
        self.can_accept_connection(addr)
            .context("Connection limit check failed")?;

        // Получаем семафор (может заблокироваться если лимит достигнут)
        let _permit = self
            .connection_semaphore
            .acquire()
            .await
            .context("Failed to acquire connection semaphore")?;

        // Увеличиваем счетчики
        self.increment_ip_connections(addr);
        let connection_count = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;

        let connection_id = self.connection_counter.fetch_add(1, Ordering::Relaxed) + 1;

        info!(
            "Connection {} established from {} (active: {})",
            connection_id, addr, connection_count
        );

        // Создаем обработчик соединения
        let handler = ConnectionHandler::new(
            connection_id,
            socket,
            addr,
            engine,
            self.config.clone(),
            self.shutdown_signal.clone(),
        );

        let result = handler.run().await;

        // Уменьшаем счетчики при завершении
        self.decrement_ip_connections(addr);
        let remaining_connections = self.active_connections.fetch_sub(1, Ordering::Relaxed) - 1;

        match &result {
            Ok(_) => debug!(
                "Connection {} from {} closed gracefully (remaining: {})",
                connection_id, addr, remaining_connections
            ),
            Err(e) => error!(
                "Connection {} from {} closed with error: {} (remaining: {})",
                connection_id, addr, e, remaining_connections
            ),
        }

        result
    }

    /// Проверить, можно ли принять новое соединение с данного IP
    fn can_accept_connection(
        &self,
        addr: SocketAddr,
    ) -> Result<()> {
        // проверяем общий лимит
        if self.connection_semaphore.available_permits() == 0 {
            return Err(anyhow!("Maximum connections limit reached"));
        }

        // проверяем лимит по IP
        let ip = addr.ip();
        let ip_connections = self.ip_connections.read().unwrap();

        if let Some(counter) = ip_connections.get(&ip) {
            let current_count = counter.load(Ordering::Relaxed);
            if current_count >= self.config.max_connections_per_ip as u32 {
                return Err(anyhow!("Too many connections from IP: {ip}"));
            }
        }

        Ok(())
    }

    /// Увеличить счётчик соединений для IP
    fn increment_ip_connections(
        &self,
        addr: SocketAddr,
    ) {
        let ip = addr.ip();
        let mut ip_connections = self.ip_connections.write().unwrap();

        ip_connections
            .entry(ip)
            .or_insert_with(|| AtomicU32::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Уменьшить счётчик соединений для IP
    fn decrement_ip_connections(
        &self,
        addr: SocketAddr,
    ) {
        let ip = addr.ip();
        let mut ip_connections = self.ip_connections.write().unwrap();

        if let Some(counter) = ip_connections.get(&ip) {
            let new_count = counter.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);

            // удаляем запись если соединение больше нет
            if new_count == 0 {
                ip_connections.remove(&ip);
            }
        }
    }
}

impl ConnectionHandler {
    fn new(
        connection_id: u32,
        socket: TcpStream,
        addr: SocketAddr,
        engine: Arc<StorageEngine>,
        config: ConnectionConfig,
        shutdown_signal: Arc<tokio::sync::Notify>,
    ) -> Self {
        // Разделяем socket на части для чтения и записи
        let (read_half, write_half) = socket.into_split();
        let reader = BufReader::with_capacity(config.read_buffer_size, read_half);

        Self {
            connection_id,
            reader,
            writer: write_half,
            addr,
            engine,
            config,
            shutdown_signal,
            last_activity: Instant::now(),
        }
    }

    /// Основной цикл обработки соединения.
    /// Основной цикл обработки соединения.
    async fn run(self) -> Result<()> {
        // Деструктурируем self чтобы избежать проблем с частичным перемещением
        let ConnectionHandler {
            connection_id,
            reader,
            mut writer,
            addr,
            engine,
            config,
            shutdown_signal,
            mut last_activity,
        } = self;

        let mut lines = reader.lines();

        loop {
            select! {
                // Проверяем сигнал shutdown
                _ = shutdown_signal.notified() => {
                    info!("Connection {} ({}): Received shutdown signal", connection_id, addr);
                    Self::send_response_to_writer(&mut writer, "-ERR Server shutting down\r\n", config.write_timeout).await?;
                    break;
                }

                // Проверяем таймаут простоя
                _ = sleep(config.idle_timeout) => {
                    if last_activity.elapsed() >= config.idle_timeout {
                        warn!("Connection {} ({}): Idle timeout", connection_id, addr);
                        Self::send_response_to_writer(&mut writer, "-ERR Connection idle timeout\r\n", config.write_timeout).await?;
                        break;
                    }
                }

                // Читаем команду с таймаутом
                line_result = timeout(config.read_timeout, lines.next_line()) => {
                    match line_result {
                        Ok(Ok(Some(line))) => {
                            last_activity = Instant::now();
                            trace!("Connection {} ({}): Received command: {}", connection_id, addr, line.trim());

                            match Self::process_command(&engine, &line) {
                                Ok(response) => {
                                    if let Err(e) = Self::send_response_to_writer(&mut writer, &response, config.write_timeout).await {
                                        error!("Connection {} ({}): Failed to send response: {}", connection_id, addr, e);
                                        break;
                                    }

                                    // Проверяем, была ли это команда QUIT
                                    if response == "+OK\r\n" && line.trim().to_uppercase() == "QUIT" {
                                        info!("Connection {} ({}): Client sent QUIT, closing", connection_id, addr);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Connection {} ({}): Command processing error: {}", connection_id, addr, e);
                                    if let Err(e) = Self::send_response_to_writer(&mut writer, "-ERR Internal server error\r\n", config.write_timeout).await {
                                        error!("Connection {} ({}): Failed to send error response: {}", connection_id, addr, e);
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(Ok(None)) => {
                            // Клиент закрыл соединение
                            debug!("Connection {} ({}): Client closed connection", connection_id, addr);
                            break;
                        }
                        Ok(Err(e)) => {
                            if e.kind() == ErrorKind::InvalidData {
                                warn!("Connection {} ({}): Ignoring invalid UTF-8 from client", connection_id, addr);
                                continue; // не рвём соединение
                            }
                            if Self::is_recoverable_error(&e) {
                                debug!("Connection {} ({}): Recoverable error: {}", connection_id, addr, e);
                                break;
                            } else {
                                error!("Connection {} ({}): Fatal read error: {}", connection_id, addr, e);
                                return Err(e.into());
                            }
                        }
                        Err(_) => {
                            warn!("Connection {} ({}): Read timeout", connection_id, addr);
                            if let Err(e) = Self::send_response_to_writer(&mut writer, "-ERR Read timeout\r\n", config.write_timeout).await {
                                error!("Connection {} ({}): Failed to send timeout response: {}", connection_id, addr, e);
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Graceful close
        Self::graceful_close_writer(connection_id, writer).await
    }

    /// Обрабатывает команду (статический метод)
    fn process_command(
        engine: &Arc<StorageEngine>,
        line: &str,
    ) -> Result<String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(String::new());
        }

        let response = match parts[0].to_uppercase().as_str() {
            "PING" => {
                if parts.len() == 1 {
                    "+PONG\r\n".to_string()
                } else {
                    format!("+{}\r\n", parts[1])
                }
            }
            "QUIT" => "+OK\r\n".to_string(),
            "SET" if parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let v = Value::Str(Sds::from(parts[2].as_bytes()));
                match engine.set(&k, v) {
                    Ok(_) => "+OK\r\n".to_string(),
                    Err(e) => {
                        error!("SET command failed: {}", e);
                        "-ERR SET failed\r\n".to_string()
                    }
                }
            }
            "GET" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.get(&k) {
                    Ok(Some(Value::Str(s))) => match String::from_utf8(s.to_vec()) {
                        Ok(s) => format!("+{}\r\n", s),
                        Err(_) => "-ERR Invalid UTF-8\r\n".to_string(),
                    },
                    Ok(Some(_)) => "-ERR Unsupported type\r\n".to_string(),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(e) => {
                        error!("GET command failed: {}", e);
                        "-ERR GET failed\r\n".to_string()
                    }
                }
            }
            "DEL" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.del(&k) {
                    Ok(true) => ":1\r\n".to_string(),
                    Ok(false) => ":0\r\n".to_string(),
                    Err(e) => {
                        error!("DEL command failed: {}", e);
                        "-ERR DEL failed\r\n".to_string()
                    }
                }
            }
            "MSET" if parts.len() > 1 => {
                let args = &parts[1..];
                if !args.len().is_multiple_of(2) {
                    "-ERR Wrong number of arguments for MSET\r\n".to_string()
                } else {
                    let mut all_success = true;
                    for chunk in args.chunks(2) {
                        let k = Sds::from(chunk[0].as_bytes());
                        let v = Value::Str(Sds::from(chunk[1].as_bytes()));
                        if let Err(e) = engine.set(&k, v) {
                            error!("MSET command failed on key {}: {}", chunk[0], e);
                            all_success = false;
                            break;
                        }
                    }
                    if all_success {
                        "+OK\r\n".to_string()
                    } else {
                        "-ERR MSET failed\r\n".to_string()
                    }
                }
            }
            "MGET" if parts.len() > 1 => {
                let sds_keys: Vec<Sds> = parts[1..]
                    .iter()
                    .map(|&k| Sds::from(k.as_bytes()))
                    .collect();
                let refs: Vec<&Sds> = sds_keys.iter().collect();
                match engine.mget(&refs) {
                    Ok(vals) => {
                        let mut resp = format!("*{}\r\n", vals.len());
                        for opt in vals {
                            if let Some(Value::Str(s)) = opt {
                                let b = s.as_bytes();
                                resp += &format!("${}\r\n", b.len());
                                resp += &String::from_utf8_lossy(b);
                                resp += "\r\n";
                            } else {
                                resp += "$-1\r\n";
                            }
                        }
                        resp
                    }
                    Err(e) => {
                        error!("MGET command failed: {}", e);
                        "-ERR MGET failed\r\n".to_string()
                    }
                }
            }
            "GEOADD" if parts.len() == 5 => {
                let k = Sds::from(parts[1].as_bytes());
                let lon: f64 = parts[2].parse().unwrap_or(0.0);
                let lat: f64 = parts[3].parse().unwrap_or(0.0);
                let m = Sds::from(parts[4].as_bytes());
                match engine.geo_add(&k, lon, lat, &m) {
                    Ok(true) => ":1\r\n".to_string(),
                    Ok(false) => ":0\r\n".to_string(),
                    Err(e) => {
                        error!("GEOADD command failed: {}", e);
                        "-ERR GEOADD failed\r\n".to_string()
                    }
                }
            }
            "GEOPOS" if parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let m = Sds::from(parts[2].as_bytes());
                match engine.geo_pos(&k, &m) {
                    Ok(Some(pt)) => format!(
                        "*2\r\n${}\r\n{}\r\n${}\r\n{}\r\n",
                        pt.lon.to_string().len(),
                        pt.lon,
                        pt.lat.to_string().len(),
                        pt.lat
                    ),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(e) => {
                        error!("GEOPOS command failed: {}", e);
                        "-ERR GEOPOS failed\r\n".to_string()
                    }
                }
            }
            "GEODIST" if parts.len() == 4 || parts.len() == 5 => {
                let k = Sds::from(parts[1].as_bytes());
                let m1 = Sds::from(parts[2].as_bytes());
                let m2 = Sds::from(parts[3].as_bytes());
                let unit = parts.get(4).copied().unwrap_or("m");
                match engine.geo_dist(&k, &m1, &m2, unit) {
                    Ok(Some(d)) => format!("+{}\r\n", d),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(e) => {
                        error!("GEODIST command failed: {}", e);
                        "-ERR GEODIST failed\r\n".to_string()
                    }
                }
            }
            _ => "-ERR Unknown command\r\n".to_string(),
        };

        Ok(response)
    }

    /// Отправляет ответ с таймаутом (статический метод)
    async fn send_response_to_writer(
        writer: &mut OwnedWriteHalf,
        response: &str,
        write_timeout: Duration,
    ) -> Result<()> {
        timeout(write_timeout, writer.write_all(response.as_bytes()))
            .await
            .context("Write timeout")?
            .context("Failed to write response")?;

        Ok(())
    }

    /// Проверяет, является ли ошибка восстанавливаемой
    fn is_recoverable_error(error: &std::io::Error) -> bool {
        matches!(
            error.kind(),
            ErrorKind::InvalidData
                | ErrorKind::UnexpectedEof
                | ErrorKind::BrokenPipe
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::TimedOut
        )
    }

    /// Graceful закрытие соединения
    async fn graceful_close_writer(
        connection_id: u32,
        mut writer: OwnedWriteHalf,
    ) -> Result<()> {
        if let Err(e) = writer.shutdown().await {
            // Игнорируем ошибки при закрытии уже закрытого соединения
            if e.kind() != ErrorKind::NotConnected {
                debug!("Connection {}: Error during shutdown: {}", connection_id, e);
            }
        }
        debug!("Connection {} closed gracefully", connection_id);
        Ok(())
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 10000,
            max_connections_per_ip: 100,
            idle_timeout: Duration::from_secs(300),
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(10),
            read_buffer_size: 8192,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::InMemoryStore;

    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::{
        net::{TcpListener, TcpStream},
        time::Duration,
    };

    #[tokio::test(flavor = "current_thread")]
    #[allow(clippy::arc_with_non_send_sync)]
    async fn handler_run_ping_and_quit() -> anyhow::Result<()> {
        let cfg = ConnectionConfig {
            read_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            ..Default::default()
        };

        let engine = Arc::new(StorageEngine::Memory(InMemoryStore::new()));

        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let local_addr = listener.local_addr()?; // адрес для клиента

        // Серверная логика как future
        let engine_server = engine.clone();
        let cfg_server = cfg.clone();
        let server_fut = async move {
            // accept ждёт клиента
            let (socket, addr) = listener.accept().await?;
            let shutdown_notify = Arc::new(tokio::sync::Notify::new());
            let handler =
                ConnectionHandler::new(1, socket, addr, engine_server, cfg_server, shutdown_notify);
            handler.run().await?; // выполняем обработчик в этом же потоке
            Ok::<(), anyhow::Error>(())
        };

        // Клиентская логика как future
        let client_fut = async move {
            let mut client = TcpStream::connect(local_addr).await?;
            client.write_all(b"PING\r\n").await?; // requires AsyncWriteExt
            let mut buf = vec![0u8; 128];
            let n = client.read(&mut buf).await?; // requires AsyncReadExt
            let got = String::from_utf8_lossy(&buf[..n]);
            assert!(got.contains("+PONG"));

            client.write_all(b"QUIT\r\n").await?;
            let n = client.read(&mut buf).await?;
            let got = String::from_utf8_lossy(&buf[..n]);
            assert!(got.contains("+OK"));

            Ok::<(), anyhow::Error>(())
        };

        // Запускаем оба future параллельно на одном (current_thread) рантайме — не требуется Send.
        tokio::try_join!(server_fut, client_fut)?;
        Ok(())
    }
}
