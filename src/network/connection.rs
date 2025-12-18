use std::{
    borrow::Cow,
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
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedWriteHalf, TcpStream},
    select,
    sync::Semaphore,
    time::{sleep, timeout, Instant},
};
use tracing::{debug, error, info, trace, warn};

use crate::{
    network::{
        connection_registry::ConnectionRegistry,
        connection_state::{ConnectionInfo, ConnectionState},
    },
    zsp::{ZspDecoder, ZspEncoder, ZspFrame},
    Sds, StorageEngine, Value,
};

/// Конфигурация для обработки соединений.
///
/// Используется `ConnectionManager` и `ConnectionHandler` для настройки
/// лимитов, таймаутов и размеров буферов.
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

/// Менеджер соединений.
///
/// Обеспечивает безопасную работу с TCP-соединениями, защиту от DoS
/// и поддержку graceful shutdown.
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
    /// Реестр активных соединений (NEW)
    registry: Arc<ConnectionRegistry>,
}

/// Обработчик отдельного соединения.
///
/// Инкапсулирует логику чтения и записи, таймаутов, протоколов
/// (текстовый и ZSP) и обновления статистики.
pub struct ConnectionHandler {
    /// Уникальный идентификатор соединения.
    connection_id: u32,
    /// Буфер для чтения данных.
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    /// Половина сокета для записи.
    writer: OwnedWriteHalf,
    /// Адрес клиента.
    addr: SocketAddr,
    /// Ссылка на движок хранения
    engine: Arc<StorageEngine>,
    /// Конфигурация соединения
    config: ConnectionConfig,
    /// Сигнал для gracefull shutdown
    shutdown_signal: Arc<tokio::sync::Notify>,
    /// Время последней активности
    last_activity: Instant,
    /// Декодер ZSP протокола
    decoder: ZspDecoder<'static>,
    /// Буфер принятых данных
    recv_buf: Vec<u8>,
    /// Информация о соединении
    connection_info: Arc<ConnectionInfo>,
}

/// Контекст обработки соединения.
///
/// Используется внутри `ConnectionHandler` для передачи движка, конфига
/// и информации о соединении.
struct ProcessContext<'a> {
    /// Движок хранения данных.
    engine: &'a Arc<StorageEngine>,
    /// Конфигурация соединения.
    config: &'a ConnectionConfig,
    /// Информация о соединении.
    connection_info: &'a Arc<ConnectionInfo>,
    /// Идентификатор соединения.
    connection_id: u32,
    /// Адрес клиента.
    addr: SocketAddr,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl ConnectionManager {
    /// Создаёт новый менеджер соединений с заданной конфигурацией.
    ///
    /// # Возвращает
    /// - `Self` - инициализированный `ConnectionManager`
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            connection_semaphore: Arc::new(Semaphore::new(config.max_connections)),
            config,
            ip_connections: Arc::new(RwLock::new(HashMap::new())),
            active_connections: Arc::new(AtomicUsize::new(0)),
            shutdown_signal: Arc::new(tokio::sync::Notify::new()),
            registry: Arc::new(ConnectionRegistry::new()),
        }
    }

    /// Возвращает текущее количество активных соединений.
    ///
    /// # Возвращает
    /// - `usize` - количество активных соединений
    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Получить реестр активных соединений.
    ///
    /// # Возвращает
    /// - `&Arc<ConnectionRegistry>` - ссылка на реестр соединений
    pub fn registry(&self) -> &Arc<ConnectionRegistry> {
        &self.registry
    }

    /// Инициализация graceful shutdown для всех соединений.
    ///
    /// Уведомляет все обработчики соединений о необходимости завершить работу.
    pub fn shutdown(&self) {
        info!("Initiating graceful shutdown for connection manager");
        self.shutdown_signal.notify_waiters();
    }

    /// Ожидает завершения всех активных соединений.
    ///
    /// # Возвращает
    /// - `Ok(())` если все соединения закрыты корректно
    /// - `Err(anyhow::Error)` если таймаут превышен
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

        info!("All connections closed gracefully");
        Ok(())
    }

    /// Обрабатывает новое входящее соединение.
    ///
    /// # Возвращает
    /// - `Ok(())` если соединение закрыто корректно
    /// - `Err(anyhow::Error)` в случае ошибок во время обработки
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

        // Регистрируем соединение в реестре (NEW)
        let (connection_id, connection_info) = self.registry.register(addr);

        // Увеличиваем счетчики
        self.increment_ip_connections(addr);
        let connection_count = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;

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
            connection_info,
        );

        let result = handler.run().await;

        // Удаляем из реестра (NEW)
        self.registry.unregister(connection_id);

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

    /// Проверяет, можно ли принять новое соединение с данного IP.
    ///
    /// # Возвращает
    /// - `Ok(())` — если соединение можно принять.
    /// - `Err(anyhow::Error)` — если превышен общий лимит соединений или лимит
    ///   по IP.
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

    /// Увеличивает счётчик активных соединений для указанного IP.
    ///
    /// Если для IP ещё нет записи, создаётся новая с начальным значением 0,
    /// затем увеличивается на 1.
    ///
    /// # Примечания
    /// - Используется `RwLock` для потокобезопасного доступа к мапе
    ///   `ip_connections`.
    /// - `AtomicU32` обеспечивает безопасное увеличение счётчика без блокировок
    ///   на уровне каждого IP.
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

    /// Уменьшает счётчик активных соединений для указанного IP.
    ///
    /// Если после уменьшения счётчик достигает нуля, запись для IP удаляется из
    /// мапы.
    ///
    /// # Примечания
    /// - Используется `RwLock` для потокобезопасного доступа к мапе
    ///   `ip_connections`.
    /// - `AtomicU32` обеспечивает безопасное уменьшение счётчика без блокировок
    ///   на уровне каждого IP.
    /// - Метод корректно обрабатывает случай, когда соединений для IP больше
    ///   нет.
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
    /// Создаёт новый обработчик соединения.
    ///
    /// # Возвращает
    /// - `Self` - инициализированный обработчик соединения
    fn new(
        connection_id: u32,
        socket: TcpStream,
        addr: SocketAddr,
        engine: Arc<StorageEngine>,
        config: ConnectionConfig,
        shutdown_signal: Arc<tokio::sync::Notify>,
        connection_info: Arc<ConnectionInfo>,
    ) -> Self {
        // Разделяем socket на части для чтения и записи
        let (read_half, write_half) = socket.into_split();
        let reader = BufReader::with_capacity(config.read_buffer_size, read_half);

        // Устанавливаем начальное состояние (NEW)
        connection_info.set_state(ConnectionState::Idle);

        Self {
            connection_id,
            reader,
            writer: write_half,
            addr,
            engine,
            config,
            shutdown_signal,
            last_activity: Instant::now(),
            decoder: ZspDecoder::new(),
            recv_buf: Vec::new(),
            connection_info,
        }
    }

    /// Основной цикл обработки соединения.
    ///
    /// Выполняет чтение, обработку команд (текстовых и ZSP),
    /// управление таймаутами и graceful shutdown.
    ///
    /// # Возвращает
    /// - `Ok(())` если соединение завершено корректно
    /// - `Err(anyhow::Error)` в случае ошибки при чтении или обработке команд
    async fn run(mut self) -> Result<()> {
        let connection_id = self.connection_id;
        let addr = self.addr;
        let shutdown = self.shutdown_signal.clone();
        let mut last_activity = self.last_activity;

        let ctx = ProcessContext {
            engine: &self.engine,
            config: &self.config,
            connection_info: &self.connection_info,
            connection_id,
            addr,
        };

        // Временный буфер для чтения
        let mut tmp = vec![0u8; self.config.read_buffer_size];

        loop {
            select! {
                _ = shutdown.notified() => {
                    info!("Connection {} ({}): Received shutdown signal", connection_id, addr);
                    ctx.connection_info.set_state(ConnectionState::Closing);
                    Self::send_response_to_writer(&mut self.writer, "-ERR Server shutting down\r\n", ctx.config.write_timeout).await?;
                    break;
                }

                _ = sleep(ctx.config.idle_timeout) => {
                    if last_activity.elapsed() >= ctx.config.idle_timeout {
                        warn!("Connection {} ({}): Idle timeout", connection_id, addr);
                        ctx.connection_info.set_state(ConnectionState::Closing);
                        Self::send_response_to_writer(&mut self.writer, "-ERR Connection idle timeout\r\n", ctx.config.write_timeout).await?;
                        break;
                    }
                }

                read_res = timeout(ctx.config.read_timeout, self.reader.read(&mut tmp)) => {
                    match read_res {
                        Ok(Ok(0)) => {
                            debug!("Connection {} ({}): Client closed connection", connection_id, addr);
                            ctx.connection_info.set_state(ConnectionState::Closing);
                            break;
                        }
                        Ok(Ok(n)) => {
                            last_activity = Instant::now();
                            ctx.connection_info.update_activity();

                            // добавляем прочитанные байты
                            self.recv_buf.extend_from_slice(&tmp[..n]);

                            // Обработка команд (текстовый или ZSP протокол)
                            if let Err(e) = Self::process_buffer(
                                &mut self.recv_buf,
                                &mut self.decoder,
                                &mut self.writer,
                                &ctx,
                                n as u64,
                            ).await {
                                error!("Connection {} ({}): Processing error: {}", connection_id, addr, e);
                                ctx.connection_info.record_error();
                                ctx.connection_info.set_state(ConnectionState::Closing);
                                break;
                            }
                        }
                        Ok(Err(e)) => {
                            if e.kind() == ErrorKind::InvalidData {
                                warn!("Connection {} ({}): Ignoring invalid UTF-8 from client", connection_id, addr);
                                continue;
                            }
                            if Self::is_recoverable_error(&e) {
                                debug!("Connection {} ({}): Recoverable error: {}", connection_id, addr, e);
                                ctx.connection_info.set_state(ConnectionState::Closing);
                                break;
                            } else {
                                error!("Connection {} ({}): Fatal read error: {}", connection_id, addr, e);
                                ctx.connection_info.record_error();
                                ctx.connection_info.set_state(ConnectionState::Closing);
                                return Err(e.into());
                            }
                        }
                        Err(_) => {
                            warn!("Connection {} ({}): Read timeout", connection_id, addr);
                            ctx.connection_info.record_error();
                            if let Err(e) = Self::send_response_to_writer(&mut self.writer, "-ERR Read timeout\r\n", ctx.config.write_timeout).await {
                                error!("Connection {} ({}): Failed to send timeout response: {}", connection_id, addr, e);
                            }
                            ctx.connection_info.set_state(ConnectionState::Closing);
                            break;
                        }
                    }
                }
            }
        }

        // Перемещаем writer единожды при завершении (self больше не используется)
        Self::graceful_close_writer(connection_id, self.writer).await
    }

    /// Обрабатывает буфер с данными от клиента.
    ///
    /// # Возвращает
    /// - `Ok(())` если данные обработаны успешно
    /// - `Err(anyhow::Error)` если произошла ошибка обработки
    async fn process_buffer(
        recv_buf: &mut Vec<u8>,
        decoder: &mut ZspDecoder<'static>,
        writer: &mut OwnedWriteHalf,
        ctx: &ProcessContext<'_>,
        bytes_received: u64,
    ) -> Result<()> {
        if recv_buf.is_empty() {
            return Ok(());
        }

        let first = recv_buf[0];
        let is_zsp = matches!(
            first,
            b'+' | b'-' | b':' | b',' | b'#' | b'$' | b'_' | b'*' | b'%' | b'~' | b'>' | b'^'
        );

        if !is_zsp {
            // Текстовый протокол
            if let Some(pos) = recv_buf.iter().position(|&b| b == b'\n') {
                let line_bytes = recv_buf.drain(..=pos).collect::<Vec<u8>>();
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                trace!(
                    "Connection {} ({}): Received text command: {}",
                    ctx.connection_id,
                    ctx.addr,
                    line.trim()
                );

                ctx.connection_info.set_state(ConnectionState::Processing);

                match Self::process_command(ctx.engine, &line) {
                    Ok(response) => {
                        let response_bytes = response.len() as u64;
                        if let Err(e) = Self::send_response_to_writer(
                            writer,
                            &response,
                            ctx.config.write_timeout,
                        )
                        .await
                        {
                            error!(
                                "Connection {} ({}): Failed to send response: {}",
                                ctx.connection_id, ctx.addr, e
                            );
                            return Err(e);
                        }

                        // Записываем статистику
                        ctx.connection_info
                            .record_command(bytes_received, response_bytes);
                        ctx.connection_info.set_state(ConnectionState::Idle);

                        if response == "+OK\r\n" && line.trim().to_uppercase() == "QUIT" {
                            info!(
                                "Connection {} ({}): Client sent QUIT, closing",
                                ctx.connection_id, ctx.addr
                            );
                            ctx.connection_info.set_state(ConnectionState::Closing);
                            return Err(anyhow!("Client quit"));
                        }
                    }
                    Err(e) => {
                        error!(
                            "Connection {} ({}): Command processing error: {}",
                            ctx.connection_id, ctx.addr, e
                        );
                        ctx.connection_info.record_error();
                        if let Err(e) = Self::send_response_to_writer(
                            writer,
                            "-ERR Internal server error\r\n",
                            ctx.config.write_timeout,
                        )
                        .await
                        {
                            error!(
                                "Connection {} ({}): Failed to send error response: {}",
                                ctx.connection_id, ctx.addr, e
                            );
                            return Err(e);
                        }
                        ctx.connection_info.set_state(ConnectionState::Idle);
                    }
                }
            }
        } else {
            // ZSP протокол
            let boxed = recv_buf.clone().into_boxed_slice();
            let total = boxed.len();
            let leaked: &'static mut [u8] = Box::leak(boxed);
            let mut slice: &'static [u8] = &leaked[..];

            match decoder.decode(&mut slice) {
                Ok(Some(frame)) => {
                    let remaining = slice.len();
                    let consumed = total - remaining;
                    recv_buf.drain(..consumed);

                    ctx.connection_info.set_state(ConnectionState::Processing);

                    if let Err(e) = Self::handle_zsp_frame(
                        ctx.engine,
                        frame,
                        writer,
                        ctx.config,
                        ctx.connection_info,
                    )
                    .await
                    {
                        error!(
                            "Connection {} ({}): ZSP handling error: {}",
                            ctx.connection_id, ctx.addr, e
                        );
                        ctx.connection_info.record_error();
                    }

                    ctx.connection_info.set_state(ConnectionState::Idle);
                }
                Ok(None) => {
                    // Частичный фрейм, ждём больше данных
                }
                Err(e) => {
                    error!(
                        "Connection {} ({}): ZSP decode error: {}",
                        ctx.connection_id, ctx.addr, e
                    );
                    ctx.connection_info.record_error();
                    let err_frame = ZspFrame::FrameError(format!("ERR zsp decode: {}", e));
                    let enc = ZspEncoder::encode(&err_frame)
                        .map_err(|e| anyhow::anyhow!("Zsp encode failed: {}", e))?;
                    timeout(ctx.config.write_timeout, writer.write_all(&enc))
                        .await
                        .context("Write timeout")??;
                }
            }
        }

        Ok(())
    }

    /// Обрабатывает команду клиента (статический метод).
    ///
    /// Парсит строку `line`, определяет команду и её аргументы, выполняет
    /// соответствующее действие через `StorageEngine` и формирует ответ в
    /// формате протокола Redis.
    ///
    /// # Возвращает
    /// - `Ok(String)` — ответ на команду в формате протокола Redis (например,
    ///   `+OK\r\n`, `$-1\r\n` и т.д.).
    /// - `Err(anyhow::Error)` — если произошла внутренняя ошибка обработки
    ///   команды.
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
            "GEORADIUS" if parts.len() >= 5 => {
                let key = Sds::from(parts[1].as_bytes());
                let lon: f64 = parts[2].parse().unwrap_or(0.0);
                let lat: f64 = parts[3].parse().unwrap_or(0.0);
                let radius: f64 = parts[4].parse().unwrap_or(0.0);

                // unit опционально, если есть 6-й аргумент
                let unit = parts.get(5).copied().unwrap_or("km");

                match engine.geo_radius(&key, lon, lat, radius, unit) {
                    Ok(results) => {
                        let mut resp = format!("*{}\r\n", results.len());
                        for (name, distance, point) in results {
                            resp += "*3\r\n";
                            resp += &format!("${}\r\n{}\r\n", name.len(), name);
                            resp += &format!("+{}\r\n", distance);
                            resp += &format!(
                                "*2\r\n${}\r\n{}\r\n${}\r\n{}\r\n",
                                point.lon.to_string().len(),
                                point.lon,
                                point.lat.to_string().len(),
                                point.lat
                            );
                        }
                        resp
                    }
                    Err(e) => {
                        error!("GEORADIUS command failed: {}", e);
                        "-ERR GEORADIUS failed\r\n".to_string()
                    }
                }
            }
            "SADD" if parts.len() >= 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let members: Vec<Sds> =
                    parts[2..].iter().map(|s| Sds::from(s.as_bytes())).collect();
                match engine.sadd(&k, &members) {
                    Ok(added) => format!(":{}\r\n", added),
                    Err(e) => {
                        error!("SADD command failed: {e}");
                        "-ERR SADD failed\r\n".to_string()
                    }
                }
            }
            "SMEMBERS" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.smembers(&k) {
                    Ok(members) => {
                        let mut resp = format!("*{}\r\n", members.len());
                        for m in members {
                            let b = m.as_bytes();
                            resp += &format!("${}\r\n", b.len());
                            resp += &String::from_utf8_lossy(b);
                            resp += "\r\n";
                        }
                        resp
                    }
                    Err(e) => {
                        error!("SMEMBERS failed: {}", e);
                        "-ERR SMEMBERS failed\r\n".to_string()
                    }
                }
            }
            "SCARD" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.scard(&k) {
                    Ok(n) => format!(":{}\r\n", n),
                    Err(e) => {
                        error!("SCARD failed: {}", e);
                        "-ERR SCARD failed\r\n".to_string()
                    }
                }
            }
            "SISMEMBER" if parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let m = Sds::from(parts[2].as_bytes());
                match engine.sismember(&k, &m) {
                    Ok(true) => ":1\r\n".to_string(),
                    Ok(false) => ":0\r\n".to_string(),
                    Err(e) => {
                        error!("SISMEMBER failed: {}", e);
                        "-ERR SISMEMBER failed\r\n".to_string()
                    }
                }
            }
            "SREM" if parts.len() >= 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let members: Vec<Sds> =
                    parts[2..].iter().map(|s| Sds::from(s.as_bytes())).collect();
                match engine.srem(&k, &members) {
                    Ok(removed) => format!(":{}\r\n", removed),
                    Err(e) => {
                        error!("SREM failed: {}", e);
                        "-ERR SREM failed\r\n".to_string()
                    }
                }
            }
            "SRANDMEMBER" if parts.len() == 2 || parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                if parts.len() == 3 {
                    let cnt: isize = parts[2].parse().unwrap_or(1);
                    match engine.srandmember(&k, cnt) {
                        Ok(vec) => {
                            if cnt == 1 {
                                if vec.is_empty() {
                                    "$-1\r\n".to_string()
                                } else {
                                    let b = vec.into_iter().next().unwrap().as_bytes().to_vec();
                                    format!("${}\r\n{}\r\n", b.len(), String::from_utf8_lossy(&b))
                                }
                            } else {
                                // array
                                let mut resp = format!("*{}\r\n", vec.len());
                                for m in vec {
                                    let b = m.as_bytes();
                                    resp += &format!("${}\r\n", b.len());
                                    resp += &String::from_utf8_lossy(b);
                                    resp += "\r\n";
                                }
                                resp
                            }
                        }
                        Err(e) => {
                            error!("SRANDMEMBER failed: {}", e);
                            "-ERR SRANDMEMBER failed\r\n".to_string()
                        }
                    }
                } else {
                    // single
                    match engine.srandmember(&k, 1) {
                        Ok(mut v) => {
                            if v.is_empty() {
                                "$-1\r\n".to_string()
                            } else {
                                let b = v.remove(0).as_bytes().to_vec();
                                format!("${}\r\n{}\r\n", b.len(), String::from_utf8_lossy(&b))
                            }
                        }
                        Err(e) => {
                            error!("SRANDMEMBER failed: {e}");
                            "-ERR SRANDMEMBER failed\r\n".to_string()
                        }
                    }
                }
            }
            "SPOP" if parts.len() == 2 || parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let cnt = if parts.len() == 3 {
                    parts[2].parse::<isize>().unwrap_or(1)
                } else {
                    1
                };
                match engine.spop(&k, cnt) {
                    Ok(vec) => {
                        if cnt == 1 {
                            if vec.is_empty() {
                                "$-1\r\n".to_string()
                            } else {
                                let b = vec.into_iter().next().unwrap().as_bytes().to_vec();
                                format!("${}\r\n{}\r\n", b.len(), String::from_utf8_lossy(&b))
                            }
                        } else {
                            let mut resp = format!("*{}\r\n", vec.len());
                            for m in vec {
                                let b = m.as_bytes();
                                resp += &format!("${}\r\n", b.len());
                                resp += &String::from_utf8_lossy(b);
                                resp += "\r\n";
                            }
                            resp
                        }
                    }
                    Err(e) => {
                        error!("SPOP failed: {e}");
                        "-ERR SPOP failed\r\n".to_string()
                    }
                }
            }
            _ => "-ERR Unknown command\r\n".to_string(),
        };

        Ok(response)
    }

    /// Отправляет ответ клиенту с учётом таймаута записи.
    ///
    /// # Возвращает
    /// - `Ok(())` если запись успешна
    /// - `Err(anyhow::Error)` при ошибке записи или таймауте
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

    /// Проверяет, является ли ошибка ввода/вывода восстанавливаемой.
    ///
    /// # Возвращает
    /// - `true` — если ошибка считается временной и соединение/операцию можно
    ///   повторить.
    /// - `false` — если ошибка критическая и восстановление маловероятно.
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

    /// Graceful закрытие writer при завершении соединения.
    ///
    /// # Возвращает
    /// - `Ok(())` если writer закрыт корректно
    /// - `Err(anyhow::Error)` при ошибке закрытия
    async fn graceful_close_writer(
        connection_id: u32,
        mut writer: OwnedWriteHalf,
    ) -> Result<()> {
        if let Err(e) = writer.shutdown().await {
            if e.kind() != ErrorKind::NotConnected {
                debug!("Connection {}: Error during shutdown: {}", connection_id, e);
            }
        }
        debug!("Connection {} closed gracefully", connection_id);
        Ok(())
    }

    /// Обрабатывает один ZSP-фрейм от клиента.
    ///
    /// Функция:
    /// 1. Парсит команду из `frame`.
    /// 2. Выполняет команду через `StorageEngine`.
    /// 3. Кодирует и отправляет ответ обратно клиенту через `writer`.
    /// 4. Обновляет статистику и ошибки соединения в `connection_info`.
    ///
    /// # Возвращает
    /// - `Ok(())` — если фрейм обработан успешно (ответ отправлен клиенту).
    /// - `Err(anyhow::Error)` — если произошла критическая ошибка при обработке
    ///   или кодировании фрейма.
    async fn handle_zsp_frame(
        engine: &Arc<StorageEngine>,
        frame: ZspFrame<'static>,
        writer: &mut OwnedWriteHalf,
        config: &ConnectionConfig,
        connection_info: &Arc<ConnectionInfo>,
    ) -> Result<(), anyhow::Error> {
        use crate::network::zsp::protocol::parser::parse_command;

        match parse_command(frame) {
            Ok(store_cmd) => {
                let resp = execute_store_command(engine, store_cmd);
                match resp {
                    Ok(frame) => {
                        let encoded = ZspEncoder::encode(&frame)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        let bytes_sent = encoded.len() as u64;

                        timeout(config.write_timeout, writer.write_all(&encoded))
                            .await
                            .context("Write timeout")??;
                        timeout(config.write_timeout, writer.flush())
                            .await
                            .context("Write timeout")??;

                        // Записываем статистику
                        connection_info.record_command(0, bytes_sent);

                        Ok(())
                    }
                    Err(e) => {
                        connection_info.record_error();
                        let err_frame = ZspFrame::FrameError(format!("ERR exec: {}", e));
                        let enc = ZspEncoder::encode(&err_frame)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        timeout(config.write_timeout, writer.write_all(&enc))
                            .await
                            .context("Write timeout")??;
                        Ok(())
                    }
                }
            }
            Err(e) => {
                connection_info.record_error();
                let err_frame = ZspFrame::FrameError(format!("ERR parse: {}", e));
                let enc =
                    ZspEncoder::encode(&err_frame).map_err(|e| anyhow::anyhow!(e.to_string()))?;
                timeout(config.write_timeout, writer.write_all(&enc))
                    .await
                    .context("Write timeout")??;
                Ok(())
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для ConnectionConfig
////////////////////////////////////////////////////////////////////////////////

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

////////////////////////////////////////////////////////////////////////////////
// Внутренние методы и функции
////////////////////////////////////////////////////////////////////////////////

/// Выполняет команду хранилища и возвращает соответствующий ZSP-фрейм.
///
/// Функция преобразует `StoreCommand` в действие на `StorageEngine` и формирует
/// ответ в формате ZSP.
///
/// # Возвращает
/// - `Ok(ZspFrame<'static>)` — фрейм с результатом выполнения команды:
///     - `InlineString("OK")` для успешных команд типа SET/MSET
///     - `BinaryString(Some(...))` для GET с найденными значениями
///     - `BinaryString(None)` для GET с отсутствующими ключами
///     - `Integer(1|0)` для DEL в зависимости от того, был ли удалён ключ
///     - `Array([...])` для MGET с результатами по каждому ключу
///     - `FrameError` для неподдерживаемых типов или ошибок
/// - `Err(String)` — строковое представление ошибки при выполнении команды.
fn execute_store_command(
    engine: &Arc<StorageEngine>,
    cmd: crate::StoreCommand,
) -> Result<ZspFrame<'static>, String> {
    use crate::{Sds, Value};
    match cmd {
        crate::StoreCommand::Set(set) => {
            let k = Sds::from_str(&set.key);
            engine.set(&k, set.value).map_err(|e| e.to_string())?;
            Ok(ZspFrame::InlineString(Cow::Owned("OK".into())))
        }
        crate::StoreCommand::Get(get) => {
            let k = Sds::from_str(&get.key);
            match engine.get(&k).map_err(|e| e.to_string())? {
                Some(Value::Str(s)) => Ok(ZspFrame::BinaryString(Some(s.to_vec()))),
                Some(_) => Ok(ZspFrame::FrameError("ERR Unsupported type".into())),
                None => Ok(ZspFrame::BinaryString(None)),
            }
        }
        crate::StoreCommand::Del(del) => {
            let k = Sds::from_str(&del.key);
            let r = engine.del(&k).map_err(|e| e.to_string())?;
            Ok(ZspFrame::Integer(if r { 1 } else { 0 }))
        }
        crate::StoreCommand::MSet(mset) => {
            for (k_s, v) in mset.entries {
                let k = Sds::from_str(&k_s);
                engine.set(&k, v).map_err(|e| e.to_string())?;
            }
            Ok(ZspFrame::InlineString(Cow::Owned("OK".into())))
        }
        crate::StoreCommand::MGet(mget) => {
            let keys: Vec<Sds> = mget.keys.into_iter().map(|s| Sds::from_str(&s)).collect();
            let refs: Vec<&Sds> = keys.iter().collect();
            let vals = engine.mget(&refs).map_err(|e| e.to_string())?;
            {
                let arr = vals
                    .into_iter()
                    .map(|opt| match opt {
                        Some(Value::Str(s)) => ZspFrame::BinaryString(Some(s.to_vec())),
                        Some(_v) => ZspFrame::FrameError("ERR unsupported type".into()),
                        None => ZspFrame::BinaryString(None),
                    })
                    .collect();
                Ok(ZspFrame::Array(arr))
            }
        }
        _ => Ok(ZspFrame::FrameError("ERR unsupported command".into())),
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        time::Duration,
    };

    use super::*;
    use crate::InMemoryStore;

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
        let local_addr = listener.local_addr()?;

        let engine_server = engine.clone();
        let cfg_server = cfg.clone();
        let server_fut = async move {
            let (socket, addr) = listener.accept().await?;
            let shutdown_notify = Arc::new(tokio::sync::Notify::new());
            let registry = Arc::new(ConnectionRegistry::new());
            let (_, conn_info) = registry.register(addr);

            let handler = ConnectionHandler::new(
                1,
                socket,
                addr,
                engine_server,
                cfg_server,
                shutdown_notify,
                conn_info,
            );
            handler.run().await?;
            Ok::<(), anyhow::Error>(())
        };

        let client_fut = async move {
            let mut client = TcpStream::connect(local_addr).await?;
            client.write_all(b"PING\r\n").await?;
            let mut buf = vec![0u8; 128];
            let n = client.read(&mut buf).await?;
            let got = String::from_utf8_lossy(&buf[..n]);
            assert!(got.contains("+PONG"));

            client.write_all(b"QUIT\r\n").await?;
            let n = client.read(&mut buf).await?;
            let got = String::from_utf8_lossy(&buf[..n]);
            assert!(got.contains("+OK"));

            Ok::<(), anyhow::Error>(())
        };

        tokio::try_join!(server_fut, client_fut)?;
        Ok(())
    }
}
