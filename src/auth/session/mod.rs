pub mod cleanup;
pub mod config;
pub mod data;
pub mod manager;

pub use config::*;
pub use data::*;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::auth::session::manager::SessionManager;

    #[tokio::test]
    async fn test_basic_session_lifecycle() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);

        let session_id = manager
            .create_session("anton", Some("127.0.0.1".to_string()))
            .await
            .unwrap();

        // проверяем что сессия валидна
        assert!(manager
            .validate_session(&session_id, Some("127.0.0.1"))
            .await
            .is_ok());

        // проверяем что можем получить username
        let username = manager.get_username(&session_id).await.unwrap();
        assert_eq!(username, "anton");
    }

    #[tokio::test]
    async fn test_ip_validation() {
        let config = SessionConfig::builder().validate_ip(true).build();
        let manager = SessionManager::new(config);
        let session_id = manager
            .create_session("anton", Some("127.0.0.1".to_string()))
            .await
            .unwrap();

        // с правильным IP работает
        assert!(manager
            .validate_session(&session_id, Some("127.0.0.1"))
            .await
            .is_ok());

        // с другим IP - ошибка
        assert!(manager
            .validate_session(&session_id, Some("192.168.1.1"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_session_expiration() {
        tokio::time::pause();

        let config = SessionConfig::builder()
            .ttl(Duration::from_secs(10))
            .build();
        let manager = SessionManager::new(config);
        let session_id = manager
            .create_session("anton", Some("127.0.0.1".to_string()))
            .await
            .unwrap();

        // сразу работает
        assert!(manager
            .validate_session(&session_id, Some("127.0.0.1"))
            .await
            .is_ok());

        // прошло 11 секунд - сессия истекла
        tokio::time::advance(Duration::from_secs(11)).await;

        assert!(manager
            .validate_session(&session_id, Some("127.0.0.1"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_max_sessions_per_user() {
        let config = SessionConfig::builder().max_sessions_per_user(2).build();
        let manager = SessionManager::new(config);

        // Создаем 2 сессии - должно работать
        let s1 = manager
            .create_session("anton", Some("127.0.0.1".to_string()))
            .await
            .unwrap();
        let s2 = manager
            .create_session("anton", Some("127.0.0.2".to_string()))
            .await
            .unwrap();

        // Обе валидны
        assert!(manager
            .validate_session(&s1, Some("127.0.0.1"))
            .await
            .is_ok());
        assert!(manager
            .validate_session(&s2, Some("127.0.0.2"))
            .await
            .is_ok());

        // Третья сессия должна удалить самую старую
        let s3 = manager
            .create_session("anton", Some("127.0.0.3".to_string()))
            .await
            .unwrap();

        // s1 больше не валидна (самая старая удалена)
        assert!(manager
            .validate_session(&s1, Some("127.0.0.1"))
            .await
            .is_err());
        // s2 и s3 валидны
        assert!(manager
            .validate_session(&s2, Some("127.0.0.2"))
            .await
            .is_ok());
        assert!(manager
            .validate_session(&s3, Some("127.0.0.3"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_revoke_user_sessions() {
        let config = SessionConfig::default();
        let manager = SessionManager::new(config);

        let s1 = manager.create_session("anton", None).await.unwrap();
        let s2 = manager.create_session("anton", None).await.unwrap();
        let s3 = manager.create_session("boris", None).await.unwrap();

        // Отзываем все сессии anton
        manager.revoke_user_sessions("anton").await.unwrap();

        // Сессии anton невалидны
        assert!(manager.validate_session(&s1, None).await.is_err());
        assert!(manager.validate_session(&s2, None).await.is_err());

        // Сессия boris всё ещё валидна
        assert!(manager.validate_session(&s3, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_session_activity_update() {
        use std::time::Duration;

        tokio::time::pause();

        let config = SessionConfig::builder()
            .ttl(Duration::from_secs(10))
            .build();
        let manager = SessionManager::new(config);

        let session_id = manager.create_session("anton", None).await.unwrap();

        // Проходит 5 секунд
        tokio::time::advance(Duration::from_secs(5)).await;

        // Обновляем активность
        manager.validate_session(&session_id, None).await.unwrap();

        // Ещё 7 секунд (всего 12 с момента создания, но 7 с момента обновления)
        tokio::time::advance(Duration::from_secs(7)).await;

        // Сессия всё ещё валидна благодаря обновлению
        assert!(manager.validate_session(&session_id, None).await.is_ok());
    }
}
