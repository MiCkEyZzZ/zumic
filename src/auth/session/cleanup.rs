use std::{sync::Arc, time::Duration};

use tokio::{sync::RwLock, task::JoinHandle, time::interval};

use crate::engine::SessionStorage;

////////////////////////////////////////////////////////////////////////////////
// Внешние функции
////////////////////////////////////////////////////////////////////////////////

/// Запускает фоновую задачу для переодической очистки истёкших сессий.
///
/// Вовзращает `JoinHandler`, который можно использовать для отмены задачи.
pub fn spawn_cleanup_task<S>(
    storage: Arc<RwLock<S>>,
    cleanup_interval: Duration,
) -> JoinHandle<()>
where
    S: SessionStorage + 'static,
{
    tokio::spawn(async move {
        let mut ticker = interval(cleanup_interval);

        loop {
            ticker.tick().await;

            let storage = storage.read().await;
            let cleaned = storage.cleanup_expired();

            if cleaned > 0 {
                tracing::debug!("Cleaned up {} expired sessions", cleaned);
            }
        }
    })
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use tokio::time::Instant;

    use super::*;
    use crate::{
        auth::session::{SessionData, SessionId},
        InMemoryStore,
    };

    #[tokio::test]
    async fn test_cleanup_task_runs() {
        tokio::time::pause();

        let storage = Arc::new(RwLock::new(InMemoryStore::new()));

        // Создаём истёкшую сессию
        let id = SessionId::new();
        let mut data = SessionData::new("anton", None, Duration::from_secs(10));
        data.expires_at = Instant::now() - Duration::from_secs(1);

        {
            let s = storage.write().await;
            s.insert_session(id, data).unwrap();
            assert_eq!(s.len_session(), 1);
        }

        // Запускаем cleanup с интервалом 5 секунд
        let handle = spawn_cleanup_task(storage.clone(), Duration::from_secs(5));

        // Продвигаем время на 6 секунд - задача должна сработать
        tokio::time::advance(Duration::from_secs(6)).await;

        // Даём задаче немного времени на выполнение
        tokio::time::sleep(Duration::from_millis(10)).await;

        {
            let s = storage.read().await;
            assert_eq!(s.len_session(), 0); // Сессия удалена
        }

        handle.abort();
    }

    #[tokio::test]
    async fn test_cleanup_task_multiple_cycles() {
        tokio::time::pause();

        let storage = Arc::new(RwLock::new(InMemoryStore::new()));

        let handle = spawn_cleanup_task(storage.clone(), Duration::from_secs(5));

        // Добавляем сессию перед первым тиком
        {
            let s = storage.write().await;
            let mut data = SessionData::new("u1", None, Duration::from_secs(10));
            data.expires_at = Instant::now() - Duration::from_secs(1);
            s.insert_session(SessionId::new(), data).unwrap();
        }

        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        {
            let s = storage.read().await;
            assert_eq!(s.len_session(), 0);
        }

        // Добавляем ещё одну истёкшую сессию
        {
            let s = storage.write().await;
            let mut data = SessionData::new("u2", None, Duration::from_secs(10));
            data.expires_at = Instant::now() - Duration::from_secs(1);
            s.insert_session(SessionId::new(), data).unwrap();
        }

        // Второй цикл очистки
        tokio::time::advance(Duration::from_secs(5)).await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        {
            let s = storage.read().await;
            assert_eq!(s.len_session(), 0);
        }

        handle.abort();
    }
}
