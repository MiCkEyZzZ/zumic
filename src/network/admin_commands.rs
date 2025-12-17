use std::sync::Arc;

use crate::network::connection_registry::ConnectionRegistry;

/// Административные команды для управления и инспекции соединений.
///
/// Оборачивает `ConnectionRegistry` и предоставляет текстовые обработчики,
/// возвращающие строки в ZSP формате.
pub struct AdminCommands {
    registry: Arc<ConnectionRegistry>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl AdminCommands {
    /// Создаёт новый обработчик административных команд.
    ///
    /// # Возвращает
    /// - `Self` — инициализированный обработчик команд
    pub fn new(registry: Arc<ConnectionRegistry>) -> Self {
        Self { registry }
    }

    /// Возвращает список подключённых клиентов в ZSP-подобном формате.
    ///
    /// Формат совпадает с ожиданием клиентской команды `CLIENT LIST`.
    ///
    /// # Возвращает
    /// - `String` — текстовый ответ: количество элементов и последовательность
    ///   bulk-строк с информацией о каждом соединении.
    pub fn handle_client_list(&self) -> String {
        let snapshots = self.registry.all_snapshots();

        if snapshots.is_empty() {
            return "*0\r\n".to_string();
        }

        let mut response = format!("*{}\r\n", snapshots.len());

        for snapshot in snapshots {
            let info = format!(
                "id={} addr={} state={} uptime={} idle={} cmd={} sent={} recv={}{}",
                snapshot.connection_id,
                snapshot.client_addr,
                snapshot.state,
                snapshot.uptime_secs,
                snapshot.idle_secs,
                snapshot.commands_processed,
                snapshot.bytes_sent,
                snapshot.bytes_received,
                snapshot
                    .username
                    .as_ref()
                    .map(|u| format!(" user={u}"))
                    .unwrap_or_default()
            );
            response.push_str(&format!("${}\r\n{}\r\n", info.len(), info));
        }

        response
    }

    /// Возвращает подробную информацию по конкретному соединению.
    ///
    /// Если соединение найдено — возвращает многострочный human-readable блок,
    /// иначе возвращает ошибочный ответ.
    ///
    /// # Возвращает
    /// - `String` — bulk-ответ с деталями
    /// - `-ERR` строка при отсутствии соединения
    pub fn handle_client_info(
        &self,
        connection_id: u32,
    ) -> String {
        match self.registry.get_snapshot(connection_id) {
            Some(snapshot) => {
                let info = format!(
                    "Connection ID: {}\n\
                             Client Address: {}\n\
                             State: {}\n\
                             Uptime: {} seconds\n\
                             Idle Time: {} seconds\n\
                             Commands Processed: {}\n\
                             Bytes Sent: {}\n\
                             Bytes Received: {}\n\
                             Username: {}",
                    snapshot.connection_id,
                    snapshot.client_addr,
                    snapshot.state,
                    snapshot.uptime_secs,
                    snapshot.idle_secs,
                    snapshot.commands_processed,
                    snapshot.bytes_sent,
                    snapshot.bytes_received,
                    snapshot.username.unwrap_or_else(|| "N/A".to_string())
                );

                format!("${}\r\n{}\r\n", info.len(), info)
            }
            None => "-ERR Connection not found\r\n".to_string(),
        }
    }

    /// Возвращает имя пользователя, связанное с соединением.
    ///
    /// Если имя задано — возвращается bulk-строка с именем, иначе ZSP `null`.
    ///
    /// # Возвращает
    /// - `String` — bulk с именем, `"$-1\r\n"` если имя отсутствует
    /// - `-ERR` если соединения нет
    pub fn handle_client_getname(
        &self,
        connection_id: u32,
    ) -> String {
        match self.registry.get_snapshot(connection_id) {
            Some(snapshot) => {
                if let Some(username) = snapshot.username {
                    format!("${}\r\n{}\r\n", username.len(), username)
                } else {
                    "$-1\r\n".to_string()
                }
            }
            None => "-ERR Connection not found\r\n".to_string(),
        }
    }

    /// Возвращает глобальные статистики сервера в виде bulk-ответа.
    ///
    /// # Возвращает
    /// - `String` — bulk-строка с числами активных соединений, команд, байтов и
    ///   ошибок
    pub fn handle_server_stats(&self) -> String {
        let stats = self.registry.global_stats();

        let info = format!(
            "Active Connections: {}\n\
                     Total Commands: {}\n\
                     Total Bytes Sent: {}\n\
                     Total Bytes Received: {}\n\
                     Total Errors: {}",
            stats.active_connections,
            stats.total_commands,
            stats.total_bytes_sent,
            stats.total_bytes_received,
            stats.total_errors
        );

        format!("${}\r\n{}\r\n", info.len(), info)
    }

    /// Возвращает количество активных соединений в ZSP integer формате.
    ///
    /// # Возвращает
    /// - `String` — integer-ответ `:<count>\r\n`
    pub fn handle_client_count(&self) -> String {
        let count = self.registry.active_count();
        format!(":{count}\r\n")
    }

    /// Возвращает список соединений для заданного IP.
    ///
    /// Формат совпадает с `CLIENT LIST`, но ограничен соединениями с указанного
    /// IP.
    ///
    /// # Возвращает
    /// - `String` — ZSP-массив с bulk-строками по каждому соединению
    /// - `*0` если пусто
    pub fn handle_client_by_ip(
        &self,
        ip: &str,
    ) -> String {
        let snapshots = self.registry.snapshots_by_ip(ip);

        if snapshots.is_empty() {
            return "*0\r\n".to_string();
        }

        let mut response = format!("*{}\r\n", snapshots.len());

        for snapshot in snapshots {
            let info = format!(
                "id={} addr={} state={} uptime={} idle={} cmd={}",
                snapshot.connection_id,
                snapshot.client_addr,
                snapshot.state,
                snapshot.uptime_secs,
                snapshot.idle_secs,
                snapshot.commands_processed
            );

            response.push_str(&format!("${}\r\n{}\r\n", info.len(), info));
        }

        response
    }

    /// Разбирает и выполняет административную команду.
    ///
    /// Поддерживаемые вызовы:
    /// - `CLIENT LIST`, `CLIENT COUNT`, `CLIENT INFO <id>`, `CLIENT GETNAME
    ///   <id>`, `CLIENT BY IP <ip>`, `SERVER STATS`.
    ///
    /// # Возвращает
    /// - `Option<String>` — `Some(response)` если команда распознана и
    ///   обработана,
    /// - `None` если команда не относится к административным.
    pub fn execute(
        &self,
        parts: &[&str],
    ) -> Option<String> {
        if parts.is_empty() {
            return None;
        }

        match (parts[0].to_uppercase().as_str(), parts.get(1)) {
            ("CLIENT", Some(&"LIST")) => Some(self.handle_client_list()),
            ("CLIENT", Some(&"COUNT")) => Some(self.handle_client_count()),
            ("CLIENT", Some(&"INFO")) if parts.len() >= 3 => {
                if let Ok(id) = parts[2].parse::<u32>() {
                    Some(self.handle_client_info(id))
                } else {
                    Some("-ERR Invalid connection ID\r\n".to_string())
                }
            }
            ("CLIENT", Some(&"GETNAME")) if parts.len() >= 3 => {
                if let Ok(id) = parts[2].parse::<u32>() {
                    Some(self.handle_client_getname(id))
                } else {
                    Some("-ERR Invalid connection ID\r\n".to_string())
                }
            }
            ("CLIENT", Some(&"BY")) if parts.len() >= 4 && parts[2].to_uppercase() == "IP" => {
                Some(self.handle_client_by_ip(parts[3]))
            }
            ("SERVER", Some(&"STATS")) => Some(self.handle_server_stats()),
            _ => None, // Не админ-команда, пропускаем
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;

    #[test]
    fn test_client_count() {
        let registry = Arc::new(ConnectionRegistry::new());
        let admin = AdminCommands::new(registry.clone());

        assert_eq!(admin.handle_client_count(), ":0\r\n");

        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        registry.register(addr);
        registry.register(addr);

        assert_eq!(admin.handle_client_count(), ":2\r\n");
    }

    #[test]
    fn test_client_list() {
        let registry = Arc::new(ConnectionRegistry::new());
        let admin = AdminCommands::new(registry.clone());

        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let (id1, info1) = registry.register(addr);
        let (id2, info2) = registry.register(addr);

        info1.record_command(100, 200);
        info2.record_command(200, 300);

        let response = admin.handle_client_list();
        assert!(response.starts_with("*2\r\n"));
        assert!(response.contains(&format!("id={id1}")));
        assert!(response.contains(&format!("id={id2}")));
    }

    #[test]
    fn test_client_info() {
        let registry = Arc::new(ConnectionRegistry::new());
        let admin = AdminCommands::new(registry.clone());

        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let (id, info) = registry.register(addr);

        info.record_command(100, 200);

        let response = admin.handle_client_info(id);
        assert!(response.contains("Connection ID:"));
        assert!(response.contains("127.0.0.1:1234"));
        assert!(response.contains("Commands Processed: 1"));
    }

    #[test]
    fn test_server_stats() {
        let registry = Arc::new(ConnectionRegistry::new());
        let admin = AdminCommands::new(registry.clone());

        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let (_, info1) = registry.register(addr);
        let (_, info2) = registry.register(addr);

        info1.record_command(100, 200);
        info2.record_command(50, 75);

        let response = admin.handle_server_stats();
        assert!(response.contains("Active Connections: 2"));
        assert!(response.contains("Total Commands: 2"));
    }

    #[test]
    fn test_execute_command() {
        let registry = Arc::new(ConnectionRegistry::new());
        let admin = AdminCommands::new(registry.clone());

        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        registry.register(addr);

        // CLIENT COUNT
        let result = admin.execute(&["CLIENT", "COUNT"]);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), ":1\r\n");

        // CLIENT LIST
        let result = admin.execute(&["CLIENT", "LIST"]);
        assert!(result.is_some());

        // SERVER STATS
        let result = admin.execute(&["SERVER", "STATS"]);
        assert!(result.is_some());

        // Неизвестная команда
        let result = admin.execute(&["UNKNOWN"]);
        assert!(result.is_none());
    }
}
