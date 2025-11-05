use std::{net::SocketAddr, time::Duration};

use tracing::{debug, info};
use zumic_error::{ClientError, ZumicResult as ClientResult};

use crate::{
    client::ClientConnection,
    zsp::{Command, Response},
    Value,
};

/// Конфигурация клиента.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Таймаут подключения
    pub connect_timeout: Duration,
    /// Таймаут чтения
    pub read_timeout: Duration,
    /// Таймаут записи
    pub write_timeout: Duration,
    /// Пароль для аутентификации (опционально)
    pub password: Option<String>,
    /// Имя пользователя для аутентификации (опционально)
    pub username: Option<String>,
}

/// Клиент Zumic
///
/// Высокоуровневый интерфейс для взаимодействия с сервером Zumic.
/// Предоставляет удобные методы для выполнения команд.
pub struct ZumicClient {
    connection: ClientConnection,
    config: ClientConfig,
    authenticated: bool,
}

impl ZumicClient {
    /// Подключается к серверу Zumic
    pub async fn connect(
        addr: SocketAddr,
        config: ClientConfig,
    ) -> ClientResult<Self> {
        info!("Подключение к Zumic серверу: {addr}");

        let connection = ClientConnection::connect(
            addr,
            config.connect_timeout,
            config.read_timeout,
            config.write_timeout,
        )
        .await?;

        let client = Self {
            connection,
            config,
            authenticated: true,
        };

        // // Используем поле структуры
        // if client.config.password.is_none() {
        //     client.authenticate().await?;
        // }

        info!("Успешно подключён к {addr}");
        Ok(client)
    }

    /// Аутентификация на сервере
    pub async fn authenticate(&mut self) -> ClientResult<()> {
        let password =
            self.config
                .password
                .clone()
                .ok_or_else(|| ClientError::AuthenticationFailed {
                    reason: "No password provided".to_string(),
                })?;

        debug!("Аутентификация на сервере");

        let command = Command::Auth {
            user: self.config.username.clone(),
            pass: password,
        };

        match self.connection.execute_command(&command).await? {
            Response::Ok => {
                self.authenticated = true;
                info!("Аутентификация успешна");
                Ok(())
            }
            Response::Error(msg) => Err(ClientError::AuthenticationFailed {
                reason: msg.to_string(),
            }
            .into()),
            _ => Err(ClientError::UnexpectedResponse.into()),
        }
    }

    /// Проверка соединения (PING).
    pub async fn ping(&mut self) -> ClientResult<bool> {
        debug!("Отправка PING");

        let response = self.connection.execute_command(&Command::Ping).await?;

        match response {
            Response::Ok => Ok(true),
            Response::String(ref s) if s == "PONG" => Ok(true),
            // Response::Error(msg) => Err(ClientError::ServerError(msg)),
            Response::Error(msg) => Err(ClientError::ServerError {
                message: msg.to_string(),
            }
            .into()),
            _ => Err(ClientError::UnexpectedResponse.into()),
        }
    }

    pub async fn get(
        &mut self,
        key: &str,
    ) -> ClientResult<Option<Value>> {
        debug!("GET {key}");

        let command = Command::Get {
            key: key.to_string(),
        };

        let response = self.connection.execute_command(&command).await?;

        match response {
            Response::Value(val) => Ok(Some(val)),
            Response::NotFound => Ok(None),
            Response::Error(msg) => Err(ClientError::ServerError {
                message: msg.to_string(),
            }
            .into()),
            _ => Err(ClientError::UnexpectedResponse.into()),
        }
    }

    pub async fn set(
        &mut self,
        key: &str,
        value: Value,
    ) -> ClientResult<()> {
        debug!("SET {key} = {value:?}");

        let command = Command::Set {
            key: key.to_string(),
            value,
        };

        let response = self.connection.execute_command(&command).await?;

        match response {
            Response::Ok => Ok(()),
            Response::Error(msg) => Err(ClientError::ServerError {
                message: msg.to_string(),
            }
            .into()),
            _ => Err(ClientError::UnexpectedResponse.into()),
        }
    }

    pub async fn del(
        &mut self,
        key: &str,
    ) -> ClientResult<bool> {
        debug!("DEL {key}");

        let command = Command::Del {
            key: key.to_string(),
        };
        let response = self.connection.execute_command(&command).await?;

        match response {
            Response::Integer(1) => Ok(true),
            Response::Integer(0) => Ok(false),
            Response::Error(msg) => Err(ClientError::ServerError {
                message: msg.to_string(),
            }
            .into()),
            _ => Err(ClientError::UnexpectedResponse.into()),
        }
    }

    pub async fn execute(
        &mut self,
        command: Command,
    ) -> ClientResult<Response> {
        debug!("Выполнение команды: {command:?}");
        self.connection.execute_command(&command).await
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.connection.server_addr()
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    pub async fn close(self) -> ClientResult<()> {
        info!("Закрытие соединения с сервером");
        self.connection.close().await
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(10),
            password: None,
            username: None,
        }
    }
}
