use std::{net::SocketAddr, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream,
    time::timeout,
};
use tracing::{debug, trace};
use zumic_error::{ClientError, ResultExt, ZumicResult as ClientResult};

use crate::{
    zsp::{Command as ZspCommand, Response, ZspDecoder, ZspEncoder, ZspFrame},
    Sds, Value,
};

/// Клиентское соединение с сервером Zumic
///
/// Управляет TCP соединением и обменом ZSP фреймами.
/// Использует существующие `ZspEncoder` и `ZspDecoder`.
pub struct ClientConnection {
    /// Адрес сервера
    addr: SocketAddr,
    /// Читающая часть соединения с буферизацией
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    /// Пишущая часть соединения с буферизацией
    writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
    /// Декодер ZSP фреймов
    decoder: ZspDecoder<'static>,
    /// Таймаут чтения
    read_timeout: Duration,
    /// Таймаут записи
    write_timeout: Duration,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl ClientConnection {
    /// Создаёт новое соединение с сервером.
    pub async fn connect(
        addr: SocketAddr,
        connect_timeout: Duration,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> ClientResult<Self> {
        debug!("Connecting to {addr}");

        // Подключаемся с таймаутом
        let stream = timeout(connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| ClientError::ConnectionTimeout)?
            .map_err(|e| ClientError::ConnectionFailed {
                address: addr.to_string(),
                reason: e.to_string(),
            })?;
        debug!("Connection established with {addr}");

        // Разделяем stream на read/write половины
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);
        let writer = BufWriter::new(write_half);

        Ok(Self {
            addr,
            reader,
            writer,
            decoder: ZspDecoder::new(),
            read_timeout,
            write_timeout,
        })
    }

    /// Отправляет команду сервера.
    pub async fn send_command(
        &mut self,
        command: &ZspCommand,
    ) -> ClientResult<()> {
        trace!("Sending command: {command:?}");

        // Создаём фрейм из команды (нужно будет ф-я преобразования)
        let frame = command_to_frame(command)?;

        // Кодируем фрейм используя существующий ZspEncoder
        let encoded = ZspEncoder::encode(&frame).map_err(|e| ClientError::EncodingError {
            reason: e.to_string(),
        })?;

        // Отправляем с таймаутом
        timeout(self.write_timeout, self.writer.write_all(&encoded))
            .await
            .map_err(|_| ClientError::WriteTimeout)?
            .map_err(|e: std::io::Error| e)?;

        // Сбрасываем буфер
        timeout(self.write_timeout, self.writer.flush())
            .await
            .map_err(|_| ClientError::WriteTimeout)?
            .map_err(|e: std::io::Error| e)?;

        trace!("Command sent successfully");

        Ok(())
    }

    /// Получает ответ от сервера
    pub async fn receive_response(&mut self) -> ClientResult<Response> {
        trace!("Waiting for server response");

        // Буфер для накопления байт между чтениями
        let mut read_buf: Vec<u8> = Vec::with_capacity(8192);

        loop {
            // Попытка декодировать уже имеющиеся байты
            if !read_buf.is_empty() {
                let boxed = read_buf.clone().into_boxed_slice();
                let total = boxed.len();
                let leaked: &'static mut [u8] = Box::leak(boxed);
                let mut slice: &'static [u8] = &leaked[..];
                match self.decoder.decode(&mut slice) {
                    Ok(Some(frame)) => {
                        let remaining = slice.len();
                        let consumed = total - remaining;
                        // удаляем потреблённые байты (кол-во consumed)
                        read_buf.drain(..consumed);
                        trace!("Frame received: {frame:?}");
                        let response = frame_to_response(frame)?;
                        trace!("Response parsed: {response:?}");
                        return Ok(response);
                    }
                    Ok(None) => {
                        // данные неполные — нужно читать ещё
                    }
                    Err(e) => {
                        return Err(ClientError::DecodingError {
                            reason: e.to_string(),
                        }
                        .into());
                    }
                }
            }

            // Читаем новые байты с таймаутом
            let mut tmp = vec![0u8; 4096];
            let n = timeout(self.read_timeout, self.reader.read(&mut tmp))
                .await
                .map_err(|_| ClientError::ReadTimeout)?
                .map_err(|e: std::io::Error| e)?;

            if n == 0 {
                return Err(ClientError::ConnectionClosed.into());
            }

            read_buf.extend_from_slice(&tmp[..n]);

            // Если буфер очень вырос — защитный предел
            if read_buf.len() > 10 * 1024 * 1024 {
                return Err(ClientError::Protocol {
                    reason: "Frame too large (>10MB)".to_string(),
                }
                .into());
            }

            // и loop продолжится — попытка декодирования выполнится снова
        }
    }

    /// Отправляет команду и ожиддает ответ.
    pub async fn execute_command(
        &mut self,
        command: &ZspCommand,
    ) -> ClientResult<Response> {
        self.send_command(command)
            .await
            .context("Failed to send command")?;
        self.receive_response()
            .await
            .context("Failed to receive response")
    }

    /// Возвращает адрес сервера
    pub fn server_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Закрывает соединение
    pub async fn close(mut self) -> ClientResult<()> {
        debug!("Closing connection to {}", self.addr);
        self.writer
            .shutdown()
            .await
            .map_err(|e: std::io::Error| e)
            .context("Failed to shutdown connection")?;
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////
// Внутренние методы и функции
////////////////////////////////////////////////////////////////////////////////

/// Преобразует ZspCommand в ZspFrame для отправки.
fn command_to_frame(command: &ZspCommand) -> ClientResult<ZspFrame<'static>> {
    use std::borrow::Cow;

    match command {
        ZspCommand::Ping => Ok(ZspFrame::Array(vec![ZspFrame::InlineString(
            Cow::Borrowed("PING"),
        )])),
        ZspCommand::Echo(msg) => Ok(ZspFrame::Array(vec![
            ZspFrame::InlineString(Cow::Borrowed("ECHO")),
            ZspFrame::InlineString(Cow::Owned(msg.clone())),
        ])),
        ZspCommand::Get { key } => Ok(ZspFrame::Array(vec![
            ZspFrame::InlineString(Cow::Borrowed("GET")),
            ZspFrame::InlineString(Cow::Owned(key.clone())),
        ])),
        ZspCommand::Set { key, value } => {
            let value_frame = value_to_frame(value)?;
            Ok(ZspFrame::Array(vec![
                ZspFrame::InlineString(Cow::Borrowed("SET")),
                ZspFrame::InlineString(Cow::Owned(key.clone())),
                value_frame,
            ]))
        }
        ZspCommand::Del { key } => Ok(ZspFrame::Array(vec![
            ZspFrame::InlineString(Cow::Borrowed("DEL")),
            ZspFrame::InlineString(Cow::Owned(key.clone())),
        ])),
        ZspCommand::Auth { user, pass } => {
            let mut frames = vec![ZspFrame::InlineString(Cow::Borrowed("AUTH"))];
            if let Some(u) = user {
                frames.push(ZspFrame::InlineString(Cow::Owned(u.clone())));
            }
            frames.push(ZspFrame::InlineString(Cow::Owned(pass.clone())));
            Ok(ZspFrame::Array(frames))
        }
        _ => Err(ClientError::UnknownCommand {
            command: format!("{command:?}"),
        }
        .into()),
    }
}

/// Преобразует Value в ZspFrame.
fn value_to_frame(value: &Value) -> ClientResult<ZspFrame<'static>> {
    match value {
        Value::Str(s) => Ok(ZspFrame::BinaryString(Some(s.to_vec()))),
        Value::Int(i) => Ok(ZspFrame::Integer(*i)),
        Value::Float(f) => Ok(ZspFrame::Float(*f)),
        Value::Bool(b) => Ok(ZspFrame::Bool(*b)),
        Value::Null => Ok(ZspFrame::Null),
        _ => Err(ClientError::Protocol {
            reason: format!("Unsupported value type: {value:?}"),
        }
        .into()),
    }
}

/// Преобразет ZspFrame в Response.
fn frame_to_response(frame: ZspFrame) -> ClientResult<Response> {
    match frame {
        ZspFrame::InlineString(s) => {
            if s == "OK" || s == "PONG" {
                Ok(Response::Ok)
            } else {
                Ok(Response::String(s.to_string()))
            }
        }
        ZspFrame::FrameError(err) => Ok(Response::Error(err)),
        ZspFrame::Integer(n) => Ok(Response::Integer(n)),
        ZspFrame::Float(f) => Ok(Response::Float(f)),
        ZspFrame::Bool(b) => Ok(Response::Bool(b)),
        ZspFrame::Null => Ok(Response::NotFound),
        ZspFrame::BinaryString(Some(bytes)) => {
            let sds = Sds::from_vec(bytes);
            Ok(Response::Value(Value::Str(sds)))
        }
        ZspFrame::BinaryString(None) => Ok(Response::NotFound),
        _ => Err(ClientError::UnexpectedResponse.into()),
    }
}
