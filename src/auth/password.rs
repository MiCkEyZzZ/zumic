use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use rand::rngs::OsRng;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("Password hashing failed")]
    Hash,
    #[error("Password verification failed")]
    Verify,
}

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(15_000, 2, 1, None).unwrap(),
    );

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordError::Hash)
}

pub fn verify_password(hash: &str, password: &str) -> Result<bool, PasswordError> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| PasswordError::Verify)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_success() {
        let password = "my_secret_password";
        let hash = hash_password(password).expect("Hashing should succeed");
        assert!(
            verify_password(&hash, password).unwrap(),
            "The correct password should verify"
        );
    }

    #[test]
    fn test_verify_password_failure() {
        let password = "my_secret_password";
        let wrong_password = "wrong_password";
        let hash = hash_password(password).expect("Hashing should succeed");
        assert!(
            !verify_password(&hash, wrong_password).unwrap(),
            "The wrong password should not verify"
        );
    }

    #[test]
    fn test_verify_invalid_hash() {
        let invalid_hash = "invalid_hash";
        assert!(
            verify_password(&invalid_hash, "password").is_err(),
            "An invalid hash should return an error"
        );
    }
}
