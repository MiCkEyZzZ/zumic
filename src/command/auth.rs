// Copyright 2025 Zumic

use crate::{error::system::StoreError, CommandExecute, Sds, StorageEngine, Value};

#[derive(Debug)]
pub struct AuthCommand {
    pub user: String,
    pub pass: String,
}

impl CommandExecute for AuthCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let user_key = Sds::from_str(&format!("user:{}", self.user));
        match store.get(&user_key)? {
            Some(Value::Str(ref stored_password)) => {
                let stored_password = stored_password
                    .as_str()
                    .map_err(|_| StoreError::InvalidType)?;
                if stored_password == self.pass {
                    // Тут по-хорошему нужно бы выставлять состояние сессии
                    Ok(Value::Str(Sds::from_str("AUTH_OK")))
                } else {
                    Ok(Value::Str(Sds::from_str("AUTH_FAILED")))
                }
            }
            Some(_) => Err(StoreError::InvalidType),
            None => Ok(Value::Str(Sds::from_str("USER_NOT_FOUND"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    fn create_store() -> StorageEngine {
        StorageEngine::Memory(InMemoryStore::new())
    }

    #[test]
    fn test_auth_success() {
        let mut store = create_store();
        store
            .set(
                &Sds::from_str("user:admin"),
                Value::Str(Sds::from_str("secret")),
            )
            .unwrap();

        let cmd = AuthCommand {
            user: "admin".into(),
            pass: "secret".into(),
        };
        let result = cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Str(Sds::from_str("AUTH_OK")));
    }

    #[test]
    fn test_auth_wrong_password() {
        let mut store = create_store();
        store
            .set(
                &Sds::from_str("user:admin"),
                Value::Str(Sds::from_str("secret")),
            )
            .unwrap();

        let cmd = AuthCommand {
            user: "admin".into(),
            pass: "wrongpass".into(),
        };
        let result = cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Str(Sds::from_str("AUTH_FAILED")));
    }

    #[test]
    fn test_auth_user_not_found() {
        let mut store = create_store();

        let cmd = AuthCommand {
            user: "ghost".into(),
            pass: "nopass".into(),
        };
        let result = cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Str(Sds::from_str("USER_NOT_FOUND")));
    }

    #[test]
    fn test_auth_invalid_type() {
        let mut store = create_store();
        store
            .set(&Sds::from_str("user:admin"), Value::Int(42))
            .unwrap(); // должно быть строка

        let cmd = AuthCommand {
            user: "admin".into(),
            pass: "secret".into(),
        };
        let result = cmd.execute(&mut store);
        assert!(matches!(result, Err(StoreError::InvalidType)));
    }
}
