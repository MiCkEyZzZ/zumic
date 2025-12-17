use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use serde::Serialize;

/// Состояние соединения в его жизненном цикле.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConnectionState {
    /// Новое соединение, только что установлено
    New,
    /// Аутентифицировано (если требуется AUTH)
    Authenticated,
    /// Обрабатывает команду
    Processing,
    /// Простаивает, ожидает команду
    Idle,
    /// Закрывается
    Closing,
}

/// Метаданные соединения.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionMetadata {
    /// Уникальный ID соединения
    pub connection_id: u32,
    /// Адрес клиента
    pub client_addr: SocketAddr,
    /// Время подключения
    #[serde(skip)]
    pub connected_at: Instant,
    /// Текущее состояние
    pub state: ConnectionState,
    /// Количество обработанных команд
    pub commands_processed: u64,
    /// Байт отправлено
    pub bytes_sent: u64,
    /// Байт получено
    pub bytes_received: u64,
    /// Время последней активности
    #[serde(skip)]
    pub last_activity: Instant,
    /// Имя пользователя (если аутентифицирован)
    pub username: Option<String>,
}

/// Потокобезопасная статистика соединения.
#[derive(Debug)]
pub struct ConnectionStats {
    /// Счётчик обработанных команд
    pub command_processed: AtomicU64,
    /// Счётчик отправленных байт
    pub bytes_sent: AtomicU64,
    /// Счётчик полученных байт
    pub bytes_received: AtomicU64,
    /// Счётчик ошибок
    pub errors: AtomicUsize,
}

/// Snapshot ьетаданных соединения для отправки клиенту.
pub struct ConnectionSnapshot {
    pub connection_id: u32,
    pub client_addr: String,
    pub state: String,
    pub uptime_secs: u64,
    pub idle_secs: u64,
    pub commands_processed: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub username: Option<String>,
}

/// Информация о соединении для внутреннего использования.
#[derive(Debug)]
pub struct ConnectionInfo {
    pub metadata: Arc<parking_lot::RwLock<ConnectionMetadata>>,
    pub stats: Arc<ConnectionStats>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl ConnectionMetadata {
    /// Создаёт новую структуру соединения.
    ///
    /// Инициализирует состояние соединения при установлении TCP-сессии:
    /// - фиксирует время подключения,
    /// - устанавливает начальное состояние,
    /// - обнуляет счётчики активности,
    /// - не назначает пользователя.
    ///
    /// # Возвращает
    /// - `Self` - инициализированное соединение в состоянии `New`
    pub fn new(
        connection_id: u32,
        client_addr: SocketAddr,
    ) -> Self {
        let now = Instant::now();
        Self {
            connection_id,
            client_addr,
            connected_at: now,
            state: ConnectionState::New,
            commands_processed: 0,
            bytes_sent: 0,
            bytes_received: 0,
            last_activity: now,
            username: None,
        }
    }

    /// Возвращает время активности соединения.
    ///
    /// Показывает длительность, прошедшую с момента последней активности
    /// соединения (чтение или запись данных).
    ///
    /// # Возвращает
    /// - `Duration` — время, прошедшее с последней активности соединения
    ///
    /// # Примечания
    /// - Использует значение `last_activity`, а не момент установления
    ///   соединения
    /// - Может использоваться для определения неактивных (idle) соединений
    pub fn uptime(&self) -> Duration {
        self.last_activity.elapsed()
    }

    /// Возвращает время простоя соединения.
    ///
    /// Показывает длительность, прошедшую с момента последней активности
    /// соединения (чтение или запись данных).
    ///
    /// # Возвращает
    /// - `Duration` — время, прошедшее с последней активности соединения
    ///
    /// # Примечания
    /// - Используется для определения неактивных (idle) соединений
    pub fn idle_time(&self) -> Duration {
        self.last_activity.elapsed()
    }

    /// Обновляет время последней активности соединения.
    ///
    /// Вызывается при любой операции ввода-вывода или обработке команды.
    ///
    /// # Примечания
    /// - Сбрасывает таймер простоя соединения
    /// - Использует текущее системное время (`Instant::now()`)
    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Увеличивает счётчик обработанных команд.
    ///
    /// Вызывается после успешной обработки клиентской команды.
    ///
    /// # Примечания
    /// - Используется для сбора статистики и мониторинга нагрузки
    pub fn increment_commands(&mut self) {
        self.commands_processed += 1;
    }

    /// Увеличивает счётчик отправленных байт.
    ///
    /// Учитывает объём данных, переданных клиенту.
    ///
    /// # Параметры
    /// - `bytes` — количество отправленных байт
    ///
    /// # Примечания
    /// - Используется для статистики и ограничения пропускной способности
    pub fn add_bytes_sent(
        &mut self,
        bytes: u64,
    ) {
        self.bytes_sent += bytes
    }

    /// Увеличивает счётчик полученных байт.
    ///
    /// Учитывает объём данных, полученных от клиента.
    ///
    /// # Параметры
    /// - `bytes` — количество полученных байт
    ///
    /// # Примечания
    /// - Используется для статистики и анализа нагрузки
    pub fn add_bytes_received(
        &mut self,
        bytes: u64,
    ) {
        self.bytes_received += bytes;
    }

    /// Устанавливает текущее состояние соединения.
    ///
    /// Используется для отслеживания этапов жизненного цикла соединения.
    ///
    /// # Параметры
    /// - `state` — новое состояние соединения
    ///
    /// # Примечания
    /// - Не выполняет проверок допустимости перехода состояний
    pub fn set_state(
        &mut self,
        state: ConnectionState,
    ) {
        self.state = state;
    }

    /// Устанавливает имя пользователя для соединения.
    ///
    /// Помечает соединение как успешно аутентифицированное.
    ///
    /// # Параметры
    /// - `username` — имя пользователя
    ///
    /// # Примечания
    /// - Устанавливает состояние соединения в `Authenticated`
    /// - Перезаписывает ранее установленное имя пользователя
    pub fn set_username(
        &mut self,
        username: String,
    ) {
        self.username = Some(username);
        self.state = ConnectionState::Authenticated;
    }
}

impl ConnectionStats {
    /// Создаёт новую структуру статистики соединения.
    ///
    /// Инициализирует все счётчики нулевыми значениями.
    /// Структура предназначена для безопасного использования
    /// в многопоточной среде.
    ///
    /// # Возвращает
    /// - `Self` — инициализированная структура статистики соединения
    pub fn new() -> Self {
        Self {
            command_processed: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            errors: AtomicUsize::new(0),
        }
    }

    /// Увеличивает счётчик обработанных команд.
    ///
    /// Вызывается после успешной обработки клиентской команды.
    ///
    /// # Примечания
    /// - Использует атомарную операцию с порядком `Relaxed`
    /// - Предназначен для высокочастотных обновлений статистики
    pub fn increment_commands(&self) {
        self.command_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Увеличивает счётчик отправленных байт.
    ///
    /// Учитывает объём данных, переданных клиенту.
    ///
    /// # Параметры
    /// - `bytes` — количество отправленных байт
    ///
    /// # Примечания
    /// - Использует атомарную операцию с порядком `Relaxed`
    pub fn add_bytes_sent(
        &self,
        bytes: u64,
    ) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Увеличивает счётчик полученных байт.
    ///
    /// Учитывает объём данных, полученных от клиента.
    ///
    /// # Параметры
    /// - `bytes` — количество полученных байт
    ///
    /// # Примечания
    /// - Использует атомарную операцию с порядком `Relaxed`
    pub fn add_bytes_received(
        &self,
        bytes: u64,
    ) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Увеличивает счётчик ошибок соединения.
    ///
    /// Вызывается при возникновении ошибок обработки команд
    /// или ввода-вывода.
    ///
    /// # Примечания
    /// - Используется для мониторинга стабильности соединения
    /// - Применяет атомарное обновление с порядком `Relaxed`
    pub fn increment_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Возвращает количество обработанных команд.
    ///
    /// # Возвращает
    /// - `u64` — общее число успешно обработанных команд
    pub fn get_commands(&self) -> u64 {
        self.command_processed.load(Ordering::Relaxed)
    }

    /// Возвращает общее количество отправленных байт.
    ///
    /// # Возвращает
    /// - `u64` — количество байт, отправленных клиенту
    pub fn get_bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Возвращает общее количество полученных байт.
    ///
    /// # Возвращает
    /// - `u64` — количество байт, полученных от клиента
    pub fn get_bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// Возвращает количество ошибок соединения.
    ///
    /// # Возвращает
    /// - `usize` — общее количество зафиксированных ошибок
    pub fn get_errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }
}

impl ConnectionInfo {
    /// Создаёт новую структуру информации о соединении.
    ///
    /// Инициализирует:
    /// - метаданные соединения с начальным состоянием,
    /// - потокобезопасную обёртку для доступа к метаданным,
    /// - атомарную структуру статистики соединения.
    ///
    /// # Возвращает
    /// - `Self` — инициализированная структура информации о соединении
    pub fn new(
        connection_id: u32,
        client_addr: SocketAddr,
    ) -> Self {
        Self {
            metadata: Arc::new(parking_lot::RwLock::new(ConnectionMetadata::new(
                connection_id,
                client_addr,
            ))),
            stats: Arc::new(ConnectionStats::new()),
        }
    }

    /// Создаёт моментальный снимок состояния соединения.
    ///
    /// Используется для передачи клиенту или внешним системам
    /// мониторинга агрегированной информации о соединении.
    ///
    /// # Возвращает
    /// - `ConnectionSnapshot` — консистентный снимок состояния соединения
    ///
    /// # Примечания
    /// - Снимок формируется на основе текущих метаданных
    /// - Атомарная статистика не блокируется при создании снимка
    pub fn snapshot(&self) -> ConnectionSnapshot {
        let meta = self.metadata.read();
        ConnectionSnapshot::from(&*meta)
    }

    /// Обновляет время последней активности соединения.
    ///
    /// Прокси-метод для потокобезопасного обновления метаданных
    /// соединения.
    ///
    /// # Примечания
    /// - Использует write-lock для синхронизации доступа
    pub fn update_activity(&self) {
        self.metadata.write().update_activity();
    }

    /// Устанавливает текущее состояние соединения.
    ///
    /// Используется для отслеживания этапов жизненного цикла
    /// соединения в многопоточной среде.
    ///
    /// # Примечания
    /// - Операция потокобезопасна
    pub fn set_state(
        &self,
        state: ConnectionState,
    ) {
        self.metadata.write().set_state(state);
    }

    /// Регистрирует обработку клиентской команды.
    ///
    /// Обновляет:
    /// - счётчики команд,
    /// - объёмы переданных данных,
    /// - время последней активности соединения.
    ///
    /// Обновление выполняется как в метаданных соединения,
    /// так и в атомарной статистике.
    ///
    /// # Примечания
    /// - Использует write-lock для метаданных
    /// - Атомарные счётчики обновляются без блокировок
    pub fn record_command(
        &self,
        bytes_received: u64,
        bytes_sent: u64,
    ) {
        let mut meta = self.metadata.write();
        meta.increment_commands();
        meta.add_bytes_received(bytes_received);
        meta.add_bytes_sent(bytes_sent);
        meta.update_activity();

        // Также обновляем атомарные счетчики
        self.stats.increment_commands();
        self.stats.add_bytes_received(bytes_received);
        self.stats.add_bytes_sent(bytes_sent);
    }

    pub fn record_error(&self) {
        self.stats.increment_errors();
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для ConnectionState, ConnectionStats,
// ConnectionSnapshot
////////////////////////////////////////////////////////////////////////////////

impl std::fmt::Display for ConnectionState {
    /// Форматирует состояние соединения в человекочитаемую строку.
    ///
    /// Используется при:
    /// - логировании,
    /// - формировании ответов клиенту,
    /// - создании снимков состояния соединения.
    ///
    /// Строковые представления:
    /// - `new`
    /// - `authenticated`
    /// - `processing`
    /// - `idle`
    /// - `closing`
    ///
    /// # Примечания
    /// - Формат стабилен и предназначен для внешнего использования
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::New => write!(f, "new"),
            Self::Authenticated => write!(f, "authenticated"),
            Self::Processing => write!(f, "processing"),
            Self::Idle => write!(f, "idle"),
            Self::Closing => write!(f, "closing"),
        }
    }
}

impl From<&ConnectionMetadata> for ConnectionSnapshot {
    /// Создаёт снимок состояния соединения на основе метаданных.
    ///
    /// Преобразует внутреннее представление соединения
    /// в структуру, пригодную для передачи клиенту
    /// или внешним системам мониторинга.
    ///
    /// # Параметры
    /// - `meta` — ссылка на метаданные соединения
    ///
    /// # Возвращает
    /// - `ConnectionSnapshot` — сериализуемый снимок состояния соединения
    ///
    /// # Примечания
    /// - Временные значения переводятся в секунды
    /// - Сетевой адрес и состояние конвертируются в строки
    fn from(meta: &ConnectionMetadata) -> Self {
        Self {
            connection_id: meta.connection_id,
            client_addr: meta.client_addr.to_string(),
            state: meta.state.to_string(),
            uptime_secs: meta.uptime().as_secs(),
            idle_secs: meta.idle_time().as_secs(),
            commands_processed: meta.commands_processed,
            bytes_sent: meta.bytes_sent,
            bytes_received: meta.bytes_received,
            username: meta.username.clone(),
        }
    }
}

impl Default for ConnectionStats {
    /// Создаёт структуру статистики соединения
    /// с начальными нулевыми значениями.
    ///
    /// Эквивалентно вызову [`ConnectionStats::new`].
    ///
    /// # Возвращает
    /// - `Self` — инициализированная структура статистики
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

    /// Тест проверяет строковое представление состояний соединения.
    #[test]
    fn test_connection_state_display() {
        assert_eq!(ConnectionState::New.to_string(), "new");
        assert_eq!(ConnectionState::Authenticated.to_string(), "authenticated");
        assert_eq!(ConnectionState::Processing.to_string(), "processing");
        assert_eq!(ConnectionState::Idle.to_string(), "idle");
        assert_eq!(ConnectionState::Closing.to_string(), "closing");
    }

    /// Тест проверяет корректность создания метаданных соединения.
    #[test]
    fn test_connection_metadata_creation() {
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let meta = ConnectionMetadata::new(1, addr);

        assert_eq!(meta.connection_id, 1);
        assert_eq!(meta.client_addr, addr);
        assert_eq!(meta.state, ConnectionState::New);
        assert_eq!(meta.commands_processed, 0);
        assert_eq!(meta.bytes_sent, 0);
        assert_eq!(meta.bytes_received, 0);
        assert!(meta.username.is_none());
    }

    /// Тест проверяет обновление метаданных соединения.
    #[test]
    fn test_connection_metadata_updates() {
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let mut meta = ConnectionMetadata::new(1, addr);

        meta.increment_commands();
        assert_eq!(meta.commands_processed, 1);

        meta.add_bytes_sent(100);
        meta.add_bytes_received(50);
        assert_eq!(meta.bytes_sent, 100);
        assert_eq!(meta.bytes_received, 50);

        meta.set_state(ConnectionState::Processing);
        assert_eq!(meta.state, ConnectionState::Processing);

        meta.set_username("testuser".to_string());
        assert_eq!(meta.username, Some("testuser".to_string()));
        assert_eq!(meta.state, ConnectionState::Authenticated);
    }

    /// Тест проверяет работу атомарной статистики соединения.
    #[test]
    fn test_connection_stats_atomic() {
        let stats = ConnectionStats::new();

        stats.increment_commands();
        stats.increment_commands();
        assert_eq!(stats.get_commands(), 2);

        stats.add_bytes_sent(100);
        stats.add_bytes_sent(50);
        assert_eq!(stats.get_bytes_sent(), 150);

        stats.add_bytes_received(200);
        assert_eq!(stats.get_bytes_received(), 200);

        stats.increment_errors();
        assert_eq!(stats.get_errors(), 1);
    }

    /// Тест проверяет работу структуры `ConnectionInfo`.
    #[test]
    fn test_connection_info() {
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let info = ConnectionInfo::new(1, addr);

        info.set_state(ConnectionState::Processing);
        {
            let meta = info.metadata.read();
            assert_eq!(meta.state, ConnectionState::Processing);
        }

        info.record_command(50, 100);
        {
            let meta = info.metadata.read();
            assert_eq!(meta.commands_processed, 1);
            assert_eq!(meta.bytes_received, 50);
            assert_eq!(meta.bytes_sent, 100);
        }
        assert_eq!(info.stats.get_commands(), 1);
        assert_eq!(info.stats.get_bytes_received(), 50);
        assert_eq!(info.stats.get_bytes_sent(), 100);

        info.record_error();
        assert_eq!(info.stats.get_errors(), 1);
    }

    /// Тест проверяет формирование снимка состояния соединения.
    #[test]
    fn test_connection_snapshot() {
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let info = ConnectionInfo::new(42, addr);

        info.record_command(100, 200);
        info.set_state(ConnectionState::Idle);

        let snapshot = info.snapshot();
        assert_eq!(snapshot.connection_id, 42);
        assert_eq!(snapshot.client_addr, "127.0.0.1:1234");
        assert_eq!(snapshot.state, "idle");
        assert_eq!(snapshot.commands_processed, 1);
        assert_eq!(snapshot.bytes_sent, 200);
        assert_eq!(snapshot.bytes_received, 100);
    }
}
