use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use num_cpus;
use once_cell::sync::Lazy;
use rand::rngs::OsRng;

use super::errors::PasswordError;

/// Параметры Argon2 (KiB, итерации)
const MEMORY_COST_KIB: u32 = 64 * 1024; // 64 MiB
const TIME_COST: u32 = 3;

/// Один экземпляр Argon2 для всего приложения
static ARGON2: Lazy<Argon2> = Lazy::new(|| {
    // Получаем число потоков динамически
    let parallelism = num_cpus::get() as u32;

    let params = Params::new(MEMORY_COST_KIB, TIME_COST, parallelism, None)
        .expect("Неверные параметры Argon2");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
});

/// Хеширует `password`, опционально добавляя `pepper` (секрет из конфига).
pub fn hash_password(password: &str, pepper: Option<&str>) -> Result<String, PasswordError> {
    let mut pwd = String::with_capacity(password.len() + pepper.map_or(0, |p| p.len()));
    pwd.push_str(password);

    if let Some(pep) = pepper {
        pwd.push_str(pep);
    }

    let salt = SaltString::generate(&mut OsRng);

    ARGON2
        .hash_password(pwd.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordError::Hash)
}

/// Проверяет, что `password` (+ тот же `pepper`) соответствует ранее сгенерированному `hash`.
pub fn verify_password(
    hash: &str,
    password: &str,
    pepper: Option<&str>,
) -> Result<bool, PasswordError> {
    let mut pwd = String::with_capacity(password.len() + pepper.map_or(0, |p| p.len()));
    pwd.push_str(password);
    if let Some(pep) = pepper {
        pwd.push_str(pep);
    }

    let parsed = PasswordHash::new(hash).map_err(|_| PasswordError::Verify)?;
    Ok(ARGON2.verify_password(pwd.as_bytes(), &parsed).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PEPPER: &str = "super_pepper";

    /// Проверяет, что хеширование и верификация без pepper работают корректно.
    #[test]
    fn test_hash_and_verify_no_pepper() {
        let password = "password123";
        let hash = hash_password(password, None).expect("Hash should succeed");
        assert!(verify_password(&hash, password, None).unwrap());
    }

    /// Проверяет, что хеширование с pepper требует передавать тот же pepper для верификации.
    #[test]
    fn test_hash_and_verify_with_pepper() {
        let password = "password123";
        let hash = hash_password(password, Some(TEST_PEPPER)).expect("Hash should succeed");
        assert!(!verify_password(&hash, password, None).unwrap());
        assert!(verify_password(&hash, password, Some(TEST_PEPPER)).unwrap());
    }

    /// Проверяет, что верификация неверного пароля возвращает false.
    #[test]
    fn test_verify_password_failure() {
        let hash = hash_password("correct", None).unwrap();
        assert!(!verify_password(&hash, "wrong", None).unwrap());
    }

    /// Проверяет, что передача некорректного формата хеша приводит к ошибке.
    #[test]
    fn test_verify_invalid_hash() {
        assert!(verify_password("invalid_hash", "any", None).is_err());
    }
}
