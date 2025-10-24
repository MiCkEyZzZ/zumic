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
    zsp::{ZspDecoder, ZspEncoder, ZspFrame},
    Sds, StorageEngine, Value,
};

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
    decoder: ZspDecoder<'static>,
    recv_buf: Vec<u8>,
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
            decoder: ZspDecoder::new(),
            recv_buf: Vec::new(),
        }
    }

    /// Основной цикл обработки соединения.
    /// Основной цикл обработки соединения.
    async fn run(mut self) -> Result<()> {
        let connection_id = self.connection_id;
        let mut writer = self.writer;
        let addr = self.addr;
        let engine = self.engine.clone();
        let config = self.config.clone();
        let shutdown = self.shutdown_signal.clone();
        let mut last_activity = self.last_activity;

        // Временный буфер для чтения
        let mut tmp = vec![0u8; config.read_buffer_size];

        loop {
            select! {
                _ = shutdown.notified() => {
                    info!("Connection {} ({}): Received shutdown signal", connection_id, addr);
                    Self::send_response_to_writer(&mut writer, "-ERR Server shutting down\r\n", config.write_timeout).await?;
                    break;
                }

                _ = sleep(config.idle_timeout) => {
                    if last_activity.elapsed() >= config.idle_timeout {
                        warn!("Connection {} ({}): Idle timeout", connection_id, addr);
                        Self::send_response_to_writer(&mut writer, "-ERR Connection idle timeout\r\n", config.write_timeout).await?;
                        break;
                    }
                }

                read_res = timeout(config.read_timeout, self.reader.read(&mut tmp)) => {
                    match read_res {
                        Ok(Ok(0)) => {
                            debug!("Connection {} ({}): Client closed connection", connection_id, addr);
                            break;
                        }
                        Ok(Ok(n)) => {
                            last_activity = Instant::now();

                            // добавляем прочитанные байты в накопительный буфер
                            self.recv_buf.extend_from_slice(&tmp[..n]);

                            // Если первый байт явно не ZSP-тип — fallback на текстовую обработку.
                            // Типы ZSP начинаются с одного из: + - : , # $ _ * % ~ > ^
                            if self.recv_buf.is_empty() {
                                continue;
                            }

                            let first = self.recv_buf[0];
                            let is_zsp = matches!(first,
                                b'+' | b'-' | b':' | b',' | b'#' | b'$' | b'_' | b'*' | b'%' | b'~' | b'>' | b'^'
                            );

                            if !is_zsp {
                                // Текстовый inline-protocol: читаем строку до CRLF или используем весь буфер
                                // (у нас тут простой реализм: split по '\n')
                                if let Some(pos) = self.recv_buf.iter().position(|&b| b == b'\n') {
                                    // берем до pos (включая возможный '\r')
                                    let line_bytes = self.recv_buf.drain(..=pos).collect::<Vec<u8>>();
                                    let line = String::from_utf8_lossy(&line_bytes).to_string();
                                    trace!("Connection {} ({}): Received text command: {}", connection_id, addr, line.trim());

                                    match Self::process_command(&engine, &line) {
                                        Ok(response) => {
                                            if let Err(e) = Self::send_response_to_writer(&mut writer, &response, config.write_timeout).await {
                                                error!("Connection {} ({}): Failed to send response: {}", connection_id, addr, e);
                                                break;
                                            }
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
                                } else {
                                    // ещё нет конца линии — ждём следующего чтения (продолжаем loop)
                                    continue;
                                }
                            } else {
                                // Пытаемся декодировать ZSP. Для этого создаём 'static срез из накопленного буфера.
                                // (временный leak — как у client.receive_response; если нужно, улучшим позже)
                                let boxed = self.recv_buf.clone().into_boxed_slice();
                                let total = boxed.len();
                                let leaked: &'static mut [u8] = Box::leak(boxed);
                                let mut slice: &'static [u8] = &leaked[..];

                                match self.decoder.decode(&mut slice) {
                                    Ok(Some(frame)) => {
                                        // сколько байт было потреблено?
                                        let remaining = slice.len();
                                        let consumed = total - remaining;
                                        // удаляем потреблённые байты из recv_buf
                                        self.recv_buf.drain(..consumed);

                                        // Обрабатываем фрейм
                                        if let Err(e) = Self::handle_zsp_frame(&engine, frame, &mut writer, &config).await {
                                            error!("Connection {} ({}): ZSP handling error: {}", connection_id, addr, e);
                                            // отправим ZSP-ошибку назад
                                            let err_frame = ZspFrame::FrameError(format!("ERR {}", e));
                                            let enc = ZspEncoder::encode(&err_frame).map_err(|e| anyhow::anyhow!("Zsp encode failed: {}", e))?;
                                            timeout(config.write_timeout, writer.write_all(&enc)).await.context("Write timeout")??
                                        }

                                        // Возможно в recv_buf остались дополнительные фреймы — обработаем их в цикле (loop будет итеративно читать)
                                    }
                                    Ok(None) => {
                                        // частичный фрейм — ждём доп. байтов
                                        continue;
                                    }
                                    Err(e) => {
                                        // ошибка декодирования — логируем и отправляем ZSP-ошибку
                                        error!("Connection {} ({}): ZSP decode error: {}", connection_id, addr, e);
                                        let err_frame = ZspFrame::FrameError(format!("ERR zsp decode: {}", e));
                                        let enc = ZspEncoder::encode(&err_frame).map_err(|e| anyhow::anyhow!("Zsp encode failed: {}", e))?;
                                        timeout(config.write_timeout, writer.write_all(&enc)).await.context("Write timeout")??
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            if e.kind() == ErrorKind::InvalidData {
                                warn!("Connection {} ({}): Ignoring invalid UTF-8 from client", connection_id, addr);
                                continue;
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
                    } // match read_res
                } // read branch
            } // select
        } // loop

        Self::graceful_close_writer(connection_id, writer).await
    } // run

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

    async fn handle_zsp_frame(
        engine: &Arc<StorageEngine>,
        frame: ZspFrame<'static>,
        writer: &mut OwnedWriteHalf,
        config: &ConnectionConfig,
    ) -> Result<(), anyhow::Error> {
        // используем твой парсер: parse_command -> StoreCommand
        // путь к parse_command зависит от реэкспортов; если у тебя другой путь,
        // поправь.
        use crate::network::zsp::protocol::parser::parse_command;

        match parse_command(frame) {
            Ok(store_cmd) => {
                // Выполнение команды и получение ZspFrame ответа
                let resp = execute_store_command(engine, store_cmd);
                match resp {
                    Ok(frame) => {
                        let encoded = ZspEncoder::encode(&frame)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        timeout(config.write_timeout, writer.write_all(&encoded))
                            .await
                            .context("Write timeout")??;
                        timeout(config.write_timeout, writer.flush())
                            .await
                            .context("Write timeout")??;
                        Ok(())
                    }
                    Err(e) => {
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
                // ошибки парсинга возвращаем клиенту как ZSP error
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

// Вспомогательная функция: исполняет StoreCommand на engine и возвращает
// ZspFrame результат. Подправь варианты/пути StoreCommand в соответствии с
// твоим кодом, если нужно.
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
        // Добавь другие варианты по необходимости (SetNx, Rename, Auth...) или верни ошибку.
        _ => Ok(ZspFrame::FrameError("ERR unsupported command".into())),
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
    use std::sync::Arc;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        time::Duration,
    };

    use super::*;
    use crate::InMemoryStore;

    /// Тест проверяет, что обработчик соединения корректно отвечает на команду
    /// `PING` и завершает соединение после получения команды `QUIT`.
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

        // Запускаем оба future параллельно на одном (current_thread) рантайме — не
        // требуется Send.
        tokio::try_join!(server_fut, client_fut)?;
        Ok(())
    }
}
