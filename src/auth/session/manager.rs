use std::sync::Arc;

use tokio::{sync::RwLock, task::JoinHandle};
use zumic_error::SessionError;

use crate::{
    auth::session::{cleanup::spawn_cleanup_task, SessionConfig, SessionData, SessionId},
    engine::SessionStorage,
    InMemoryStore,
};

pub struct SessionManager<S = InMemoryStore>
where
    S: SessionStorage,
{
    storage: Arc<RwLock<S>>,
    config: SessionConfig,
    cleanup_handle: Option<JoinHandle<()>>,
}

////////////////////////////////////////////////////////////////////////////////
// Внутренние методы и функции
////////////////////////////////////////////////////////////////////////////////

impl SessionManager<InMemoryStore> {
    pub fn new(config: SessionConfig) -> Self {
        let storage = Arc::new(RwLock::new(InMemoryStore::new()));
        let cleanup_handle = Some(spawn_cleanup_task(storage.clone(), config.cleanup_interval));
        Self {
            storage,
            config,
            cleanup_handle,
        }
    }
}

impl<S> SessionManager<S>
where
    S: SessionStorage + 'static,
{
    pub fn with_storage(
        storage: S,
        config: SessionConfig,
    ) -> Self {
        let storage = Arc::new(RwLock::new(storage));
        let cleanup_handle = Some(spawn_cleanup_task(storage.clone(), config.cleanup_interval));
        Self {
            storage,
            config,
            cleanup_handle,
        }
    }

    pub async fn create_session(
        &self,
        username: impl Into<String>,
        ip_address: Option<String>,
    ) -> Result<SessionId, SessionError> {
        let username = username.into();

        // проверяем лимит сессий на пользователя
        if let Some(max) = self.config.max_sessions_per_user {
            let storage = self.storage.read().await;
            let mut user_sessions = storage.get_user_sessions(&username);

            if user_sessions.len() >= max {
                // Сортируем по created_at и удаляем самую старую
                user_sessions.sort_by_key(|(_, data)| data.created_at);
                if let Some((oldest_id, _)) = user_sessions.first() {
                    drop(storage);
                    let storage = self.storage.write().await;
                    storage.remove_session(oldest_id);
                }
            }
        }

        // Создаём новую сессию
        let session_id = SessionId::new();
        let session_data = SessionData::new(username, ip_address, self.config.ttl);

        let storage = self.storage.write().await;
        storage.insert_session(session_id.clone(), session_data)?;

        Ok(session_id)
    }

    pub async fn validate_session(
        &self,
        session_id: &SessionId,
        ip_address: Option<&str>,
    ) -> Result<(), SessionError> {
        let storage = self.storage.read().await;
        let mut session = storage
            .get_session(session_id)
            .ok_or(SessionError::NotFound)?;

        // проверям истечение
        if session.is_expired() {
            drop(storage);
            let storage = self.storage.write().await;
            storage.remove_session(session_id);
            return Err(SessionError::Expired);
        }

        // Проверяем IP, если включена валидация
        if self.config.validate_ip && !session.validate_ip(ip_address) {
            return Err(SessionError::IpMismatch);
        }

        // обновляем активность
        session.update_activity(self.config.ttl);
        drop(storage);

        let storage = self.storage.write().await;
        storage.insert_session(session_id.clone(), session)?;

        Ok(())
    }

    pub async fn get_username(
        &self,
        session_id: &SessionId,
    ) -> Result<String, SessionError> {
        let storage = self.storage.read().await;
        let session = storage
            .get_session(session_id)
            .ok_or(SessionError::NotFound)?;

        if session.is_expired() {
            drop(storage);
            let storage = self.storage.write().await;
            storage.remove_session(session_id);
            return Err(SessionError::Expired);
        }

        Ok(session.username)
    }

    pub async fn revoke_session(
        &self,
        session_id: &SessionId,
    ) -> Result<(), SessionError> {
        let storage = self.storage.write().await;
        storage
            .remove_session(session_id)
            .ok_or(SessionError::NotFound)?;
        Ok(())
    }

    pub async fn revoke_user_sessions(
        &self,
        username: &str,
    ) -> Result<usize, SessionError> {
        let storage = self.storage.write().await;
        let count = storage.remove_user_sessions(username);
        Ok(count)
    }

    pub async fn count_user_sessions(
        &self,
        username: &str,
    ) -> usize {
        let storage = self.storage.read().await;
        storage.get_user_sessions(username).len()
    }

    pub async fn cleanup_expired(&self) -> usize {
        let storage = self.storage.read().await;
        storage.cleanup_expired()
    }

    pub async fn total_sessions(&self) -> usize {
        let storage = self.storage.read().await;
        storage.len_session()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для SessionManager
////////////////////////////////////////////////////////////////////////////////

impl<S> Drop for SessionManager<S>
where
    S: SessionStorage,
{
    fn drop(&mut self) {
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_create_and_validate_session() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);

        let session_id = manager
            .create_session("anton", Some("127.0.0.1".to_string()))
            .await
            .unwrap();
        assert!(manager
            .validate_session(&session_id, Some("127.0.0.1"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_validate_nonexistent_session() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);
        let fake_id = SessionId::new();
        let result = manager.validate_session(&fake_id, None).await;
        assert!(matches!(result, Err(SessionError::NotFound)));
    }

    #[tokio::test]
    async fn test_ip_validation() {
        let config = SessionConfig::builder().validate_ip(true).build();
        let manager = SessionManager::new(config);

        let session_id = manager
            .create_session("anton", Some("127.0.0.1".to_string()))
            .await
            .unwrap();

        // Правильный IP
        assert!(manager
            .validate_session(&session_id, Some("127.0.0.1"))
            .await
            .is_ok());

        // Неправильный IP
        let result = manager
            .validate_session(&session_id, Some("192.168.1.1"))
            .await;
        assert!(matches!(result, Err(SessionError::IpMismatch)));
    }

    #[tokio::test]
    async fn test_session_expiration() {
        tokio::time::pause();

        let config = SessionConfig::builder()
            .ttl(Duration::from_secs(10))
            .build();
        let manager = SessionManager::new(config);

        let session_id = manager
            .create_session("anton", None::<String>)
            .await
            .unwrap();

        // Сразу валидна
        assert!(manager.validate_session(&session_id, None).await.is_ok());

        // Через 11 секунд истекла
        tokio::time::advance(Duration::from_secs(11)).await;

        let result = manager.validate_session(&session_id, None).await;
        assert!(matches!(result, Err(SessionError::Expired)));
    }

    #[tokio::test]
    async fn test_max_sessions_per_user() {
        let config = SessionConfig::builder().max_sessions_per_user(2).build();
        let manager = SessionManager::new(config);

        let s1 = manager.create_session("anton", None).await.unwrap();
        let s2 = manager.create_session("anton", None).await.unwrap();

        // Обе валидны
        assert!(manager.validate_session(&s1, None).await.is_ok());
        assert!(manager.validate_session(&s2, None).await.is_ok());

        // Третья удалит самую старую (s1)
        let s3 = manager.create_session("anton", None).await.unwrap();

        // s1 удалена
        assert!(matches!(
            manager.validate_session(&s1, None).await,
            Err(SessionError::NotFound)
        ));

        // s2 и s3 валидны
        assert!(manager.validate_session(&s2, None).await.is_ok());
        assert!(manager.validate_session(&s3, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_get_username() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);

        let session_id = manager.create_session("anton", None).await.unwrap();

        let username = manager.get_username(&session_id).await.unwrap();
        assert_eq!(username, "anton");
    }

    #[tokio::test]
    async fn test_revoke_session() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);

        let session_id = manager.create_session("anton", None).await.unwrap();

        assert!(manager.revoke_session(&session_id).await.is_ok());

        // Больше не существует
        let result = manager.validate_session(&session_id, None).await;
        assert!(matches!(result, Err(SessionError::NotFound)));
    }

    #[tokio::test]
    async fn test_revoke_user_sessions() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);

        let s1 = manager.create_session("anton", None).await.unwrap();
        let s2 = manager.create_session("anton", None).await.unwrap();
        let s3 = manager.create_session("boris", None).await.unwrap();

        let count = manager.revoke_user_sessions("anton").await.unwrap();
        assert_eq!(count, 2);

        // anton сессии удалены
        assert!(matches!(
            manager.validate_session(&s1, None).await,
            Err(SessionError::NotFound)
        ));
        assert!(matches!(
            manager.validate_session(&s2, None).await,
            Err(SessionError::NotFound)
        ));

        // boris сессия осталась
        assert!(manager.validate_session(&s3, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_count_user_sessions() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);

        assert_eq!(manager.count_user_sessions("anton").await, 0);

        manager.create_session("anton", None).await.unwrap();
        manager.create_session("anton", None).await.unwrap();

        assert_eq!(manager.count_user_sessions("anton").await, 2);
    }

    #[tokio::test]
    async fn test_activity_update_extends_session() {
        tokio::time::pause();

        let config = SessionConfig::builder()
            .ttl(Duration::from_secs(10))
            .build();
        let manager = SessionManager::new(config);

        let session_id = manager.create_session("anton", None).await.unwrap();

        // Через 5 секунд валидируем (обновляем активность)
        tokio::time::advance(Duration::from_secs(5)).await;
        assert!(manager.validate_session(&session_id, None).await.is_ok());

        // Ещё через 7 секунд (12 с момента создания, но 7 с момента обновления)
        tokio::time::advance(Duration::from_secs(7)).await;

        // Всё ещё валидна благодаря обновлению
        assert!(manager.validate_session(&session_id, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_no_ip_validation_when_disabled() {
        let config = SessionConfig::builder().validate_ip(false).build();
        let manager = SessionManager::new(config);

        let session_id = manager
            .create_session("anton", Some("127.0.0.1".to_string()))
            .await
            .unwrap();

        // Любой IP проходит
        assert!(manager
            .validate_session(&session_id, Some("192.168.1.1"))
            .await
            .is_ok());
        assert!(manager.validate_session(&session_id, None).await.is_ok());
    }
}
