use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use parking_lot::RwLock;

use crate::network::connection_state::{ConnectionInfo, ConnectionSnapshot};

/// Реестр активных соединений
///
/// Потокобезопасное хранилище всех активных соединений сервера.
/// Используется для мониторинга, статистики и административных команд.
#[derive(Debug)]
pub struct ConnectionRegistry {
    /// Хранилище соединений: connection_id -> ConnectionInfo
    connections: Arc<RwLock<HashMap<u32, Arc<ConnectionInfo>>>>,
    /// Счётчик для генерации уникальных ID
    id_counter: Arc<AtomicU32>,
}

/// Глобальная статистика по всем соединениям.
///
///
/// # Поля
/// * `active_connections` - кол-во активных соединений.
/// * `total_commands` - общее число обработанных команд.
/// * `total_bytes_sent` - общее число отправленных байт.
/// * `total_bytes_received` - общее число полученных байт.
/// * `total_errors` - общее число ошибок, зарегистрированных на соединениях.
#[derive(Debug, Clone, Copy)]
pub struct GlobalConnectionStats {
    pub active_connections: usize,
    pub total_commands: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub total_errors: usize,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl ConnectionRegistry {
    /// Создаёт новый пустой реестр.
    ///
    /// # Возвращает
    /// - Новый `ConnectionRegistry` с пустым набором соединений и счётчиком ID,
    ///   инициализированным нулём.
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            id_counter: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Регистрирует новое соединение и возвращает его ID и `ConnectionInfo`.
    ///
    /// # Возвращает
    /// - `(connection_id, Arc<ConnectionInfo>)` — сгенерированный ID и ссылку
    ///   на структуру с информацией о соединении.
    pub fn register(
        &self,
        client_addr: SocketAddr,
    ) -> (u32, Arc<ConnectionInfo>) {
        let connection_id = self.id_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let info = Arc::new(ConnectionInfo::new(connection_id, client_addr));

        self.connections.write().insert(connection_id, info.clone());

        (connection_id, info)
    }

    /// Удаляет соединение из реестра.
    ///
    /// # Примечание
    /// Если `connection_id` отсутствует — метод спокойно ничего не делает.
    pub fn unregister(
        &self,
        connection_id: u32,
    ) {
        self.connections.write().remove(&connection_id);
    }

    /// Возвращает `Arc<ConnectionInfo>` по ID.
    ///
    /// # Возвращает
    /// - `Some(Arc<ConnectionInfo>)`, если соединение найдено
    /// - `None`, если соединение не найдено
    pub fn get(
        &self,
        connection_id: u32,
    ) -> Option<Arc<ConnectionInfo>> {
        self.connections.read().get(&connection_id).cloned()
    }

    /// Возвращает количество активных соединений.
    ///
    /// # Возвращает
    /// - Текущее количество записей в реестре.
    pub fn active_count(&self) -> usize {
        self.connections.read().len()
    }

    /// Возвращает список всех активных connection IDs.
    ///
    /// # Возвращает
    /// - `Vec<u32>` с текущими ID соединений.
    pub fn active_ids(&self) -> Vec<u32> {
        self.connections.read().keys().copied().collect()
    }

    /// Возвращает snapshots всех активных соединений.
    ///
    /// # Возвращает
    /// - Вектор `ConnectionSnapshot`, сформированных по актуальным
    ///   `ConnectionInfo`.
    pub fn all_snapshots(&self) -> Vec<ConnectionSnapshot> {
        self.connections
            .read()
            .values()
            .map(|info| info.snapshot())
            .collect()
    }

    /// Возвращает snapshots соединений, отфильтрованных по предикату.
    ///
    /// # Возвращает
    /// - Вектор `ConnectionSnapshot`, для которых `predicate` возвратило
    ///   `true`.
    pub fn filter_snapshots<F>(
        &self,
        predicate: F,
    ) -> Vec<ConnectionSnapshot>
    where
        F: Fn(&ConnectionSnapshot) -> bool,
    {
        self.connections
            .read()
            .values()
            .map(|info| info.snapshot())
            .filter(|snapshot| predicate(snapshot))
            .collect()
    }

    /// Возвращает snapshot конкретного соединения по ID.
    ///
    /// # Возвращает
    /// - `Some(ConnectionSnapshot)`, если соединение найдено
    /// - `None`, если соединение не найдено
    pub fn get_snapshot(
        &self,
        connection_id: u32,
    ) -> Option<ConnectionSnapshot> {
        self.get(connection_id).map(|info| info.snapshot())
    }

    /// Возвращает snapshots соединений с указанного IP (строка-совпадение).
    ///
    /// # Возвращает
    /// - Вектор `ConnectionSnapshot` у которых `client_addr` начинается с `ip`.
    pub fn snapshots_by_ip(
        &self,
        ip: &str,
    ) -> Vec<ConnectionSnapshot> {
        self.filter_snapshots(|snapshot| snapshot.client_addr.starts_with(ip))
    }

    /// Возвращает агрегированную статистику по всем соединениям.
    ///
    /// # Возвращает
    /// - `GlobalConnectionStats` с суммарными значениями.
    pub fn global_stats(&self) -> GlobalConnectionStats {
        let connections = self.connections.read();
        let count = connections.len();

        let mut total_commands = 0u64;
        let mut total_bytes_sent = 0u64;
        let mut total_bytes_received = 0u64;
        let mut total_errors = 0usize;

        for info in connections.values() {
            total_commands += info.stats.get_commands();
            total_bytes_sent += info.stats.get_bytes_sent();
            total_bytes_received += info.stats.get_bytes_received();
            total_errors += info.stats.get_errors();
        }

        GlobalConnectionStats {
            active_connections: count,
            total_commands,
            total_bytes_sent,
            total_bytes_received,
            total_errors,
        }
    }

    /// Очищает все соединения (только для тестов или при graceful shutdown).
    ///
    /// # Примечание
    /// Метод помечен `#[cfg(test)]` — используется в unit-тестах.
    #[cfg(test)]
    pub fn clear(&self) {
        self.connections.write().clear();
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для CommandRegistry, ConnectionRegistry
////////////////////////////////////////////////////////////////////////////////

impl Default for ConnectionRegistry {
    fn default() -> Self {
        Self::new()
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
    fn test_registry_register_unregister() {
        let registry = ConnectionRegistry::new();
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();

        assert_eq!(registry.active_count(), 0);

        let (id1, _info1) = registry.register(addr);
        assert_eq!(id1, 1);
        assert_eq!(registry.active_count(), 1);

        let (id2, _info2) = registry.register(addr);
        assert_eq!(id2, 2);
        assert_eq!(registry.active_count(), 2);

        // Проверяем что можем получить соединение
        let retrieved = registry.get(id1);
        assert!(retrieved.is_some());

        // Удаляем первое соединение
        registry.unregister(id1);
        assert_eq!(registry.active_count(), 1);
        assert!(registry.get(id1).is_none());
        assert!(registry.get(id2).is_some());

        // Удаляем второе
        registry.unregister(id2);
        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn test_registry_active_ids() {
        let registry = ConnectionRegistry::new();
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();

        let (id1, _) = registry.register(addr);
        let (id2, _) = registry.register(addr);
        let (id3, _) = registry.register(addr);

        let ids = registry.active_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
        assert!(ids.contains(&id3));
    }

    #[test]
    fn test_registry_snapshots() {
        let registry = ConnectionRegistry::new();
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();

        let (id1, info1) = registry.register(addr);
        let (_id2, info2) = registry.register(addr);

        // Записываем некоторые данные
        info1.record_command(100, 200);
        info2.record_command(50, 75);

        let snapshots = registry.all_snapshots();
        assert_eq!(snapshots.len(), 2);

        // Проверяем snapshot конкретного соединения
        let snapshot1 = registry.get_snapshot(id1).unwrap();
        assert_eq!(snapshot1.connection_id, id1);
        assert_eq!(snapshot1.commands_processed, 1);
        assert_eq!(snapshot1.bytes_received, 100);
        assert_eq!(snapshot1.bytes_sent, 200);
    }

    #[test]
    fn test_registry_filter_snapshots() {
        let registry = ConnectionRegistry::new();
        let addr1: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let addr2: SocketAddr = "192.168.1.100:5678".parse().unwrap();

        let (_id1, info1) = registry.register(addr1);
        let (_id2, info2) = registry.register(addr2);
        let (_id3, info3) = registry.register(addr1);

        info1.record_command(100, 200);
        info2.record_command(50, 75);
        info3.record_command(300, 400);

        // Фильтруем по IP
        let snapshots = registry.snapshots_by_ip("127.0.0.1");
        assert_eq!(snapshots.len(), 2);

        // Фильтруем по количеству команд
        let busy_connections = registry.filter_snapshots(|s| s.commands_processed > 0);
        assert_eq!(busy_connections.len(), 3);
    }

    #[test]
    fn test_registry_global_stats() {
        let registry = ConnectionRegistry::new();
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();

        let (_id1, info1) = registry.register(addr);
        let (_id2, info2) = registry.register(addr);

        info1.record_command(100, 200);
        info1.record_command(50, 100);
        info2.record_command(75, 150);
        info1.record_error();

        let stats = registry.global_stats();
        assert_eq!(stats.active_connections, 2);
        assert_eq!(stats.total_commands, 3);
        assert_eq!(stats.total_bytes_received, 225);
        assert_eq!(stats.total_bytes_sent, 450);
        assert_eq!(stats.total_errors, 1);
    }
}
